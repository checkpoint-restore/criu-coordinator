import socket


def test_add_dependencies():
    host = "127.0.0.1"
    port = 8080

    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.connect((host, port))
    s.sendall(bytes('{'
        '"id": "kubescr", '
        '"action": "add_dependencies", '
        '"dependencies": {"c1": ["c2", "c3"], "c2": ["c1", "c3"], "c3": ["c1", "c2"]}'
    '}', "utf-8"))

    data = s.recv(1024)
    s.close()
    print('Received', repr(data))


if __name__ == '__main__':
    test_add_dependencies()
