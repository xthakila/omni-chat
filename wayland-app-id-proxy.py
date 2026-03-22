#!/usr/bin/env python3
"""
Wayland app_id proxy with FD passing support.
Intercepts xdg_toplevel.set_app_id("") and replaces with "omnichat".

Usage: python3 wayland-app-id-proxy.py <command> [args...]
"""
import os
import sys
import socket
import struct
import subprocess
import threading
import signal
import array

APP_ID = b"omnichat"
XDG_TOPLEVEL_SET_APP_ID_OPCODE = 3  # opcode 0=destroy, 1=set_parent, 2=set_title, 3=set_app_id

def recv_with_fds(sock, bufsize, maxfds=32):
    """Receive data and file descriptors via recvmsg."""
    fds = array.array("i")
    try:
        msg, ancdata, flags, addr = sock.recvmsg(
            bufsize, socket.CMSG_LEN(maxfds * fds.itemsize)
        )
    except (ConnectionResetError, BrokenPipeError, OSError):
        return b"", []

    for cmsg_level, cmsg_type, cmsg_data in ancdata:
        if cmsg_level == socket.SOL_SOCKET and cmsg_type == socket.SCM_RIGHTS:
            fds.frombytes(cmsg_data[:len(cmsg_data) - (len(cmsg_data) % fds.itemsize)])

    return msg, list(fds)

def send_with_fds(sock, data, fds=None):
    """Send data and file descriptors via sendmsg."""
    if fds:
        ancdata = [(socket.SOL_SOCKET, socket.SCM_RIGHTS,
                     array.array("i", fds).tobytes())]
    else:
        ancdata = []
    try:
        sock.sendmsg([data], ancdata)
    except (BrokenPipeError, ConnectionResetError, OSError):
        pass

def patch_app_id(data):
    """Replace empty set_app_id("") with set_app_id("omnichat")."""
    if len(data) < 12:
        return data

    result = bytearray()
    pos = 0

    while pos + 8 <= len(data):
        if pos + 8 > len(data):
            result.extend(data[pos:])
            break

        object_id = struct.unpack_from("<I", data, pos)[0]
        word2 = struct.unpack_from("<I", data, pos + 4)[0]
        opcode = word2 & 0xFFFF
        msg_size = (word2 >> 16) & 0xFFFF

        if msg_size < 8 or pos + msg_size > len(data):
            result.extend(data[pos:])
            break

        # Check for set_app_id with empty string
        # Header: 8 bytes, string_len: 4 bytes, string: 4 bytes (padded "\0")
        if opcode == XDG_TOPLEVEL_SET_APP_ID_OPCODE and msg_size >= 12:
            # Log the string content for debugging
            import sys
            str_off = pos + 8
            if str_off + 4 <= len(data):
                slen = struct.unpack_from("<I", data, str_off)[0]
                if str_off + 4 + slen <= len(data):
                    sval = data[str_off+4:str_off+4+slen-1]  # -1 for null terminator
                    sys.stderr.write(f"[proxy] opcode=2 obj={object_id} size={msg_size} str_len={slen} str='{sval.decode('utf-8','replace')}'\n")
                    sys.stderr.flush()
        if opcode == XDG_TOPLEVEL_SET_APP_ID_OPCODE and msg_size >= 12:
            str_len_off = pos + 8
            if str_len_off + 4 <= len(data):
                str_len = struct.unpack_from("<I", data, str_len_off)[0]
                if str_len == 1:  # Empty string (just \0)
                    new_str = APP_ID + b"\0"
                    new_str_len = len(new_str)
                    padded = (new_str_len + 3) & ~3
                    new_str_padded = new_str.ljust(padded, b"\0")
                    new_msg_size = 8 + 4 + padded

                    new_word2 = (opcode & 0xFFFF) | ((new_msg_size & 0xFFFF) << 16)
                    result.extend(struct.pack("<I", object_id))
                    result.extend(struct.pack("<I", new_word2))
                    result.extend(struct.pack("<I", new_str_len))
                    result.extend(new_str_padded)

                    pos += msg_size
                    continue

        result.extend(data[pos:pos + msg_size])
        pos += msg_size

    if pos < len(data):
        result.extend(data[pos:])

    return bytes(result)

def proxy_data(src, dst, fix=False):
    """Proxy data between sockets with FD passing."""
    try:
        while True:
            data, fds = recv_with_fds(src, 65536)
            if not data:
                break
            if fix:
                data = patch_app_id(data)
            send_with_fds(dst, data, fds)
            # Close forwarded FDs in our process
            for fd in fds:
                os.close(fd)
    except Exception:
        pass
    finally:
        try:
            dst.shutdown(socket.SHUT_WR)
        except OSError:
            pass

def handle_client(client, real_path):
    real = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    real.connect(real_path)

    t1 = threading.Thread(target=proxy_data, args=(client, real, True), daemon=True)
    t2 = threading.Thread(target=proxy_data, args=(real, client, False), daemon=True)
    t1.start()
    t2.start()
    t1.join()
    t2.join()
    client.close()
    real.close()

def main():
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} <command> [args...]")
        sys.exit(1)

    real_display = os.environ.get("WAYLAND_DISPLAY", "wayland-0")
    xdg_runtime = os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}")
    real_path = os.path.join(xdg_runtime, real_display)

    proxy_name = f"wayland-omnichat-{os.getpid()}"
    proxy_path = os.path.join(xdg_runtime, proxy_name)

    try:
        os.unlink(proxy_path)
    except FileNotFoundError:
        pass

    server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    server.bind(proxy_path)
    server.listen(16)

    env = os.environ.copy()
    env["WAYLAND_DISPLAY"] = proxy_name
    proc = subprocess.Popen(sys.argv[1:], env=env)

    def cleanup(sig=None, frame=None):
        proc.terminate()
        server.close()
        try:
            os.unlink(proxy_path)
        except FileNotFoundError:
            pass
        if sig:
            sys.exit(0)

    signal.signal(signal.SIGINT, cleanup)
    signal.signal(signal.SIGTERM, cleanup)

    def accept_loop():
        while True:
            try:
                client, _ = server.accept()
                threading.Thread(target=handle_client, args=(client, real_path), daemon=True).start()
            except OSError:
                break

    threading.Thread(target=accept_loop, daemon=True).start()
    proc.wait()
    cleanup()

if __name__ == "__main__":
    main()
