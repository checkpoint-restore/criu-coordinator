#include <arpa/inet.h>
#include <unistd.h>
#include <string.h>
#include <stdlib.h>
#include <stdio.h>
#include <sys/time.h>

static int main_cl(int argc, char **argv)
{
    int sk, port, ret, val = 1, rval;
    struct timeval t0, t1;
    struct sockaddr_in addr;

    sk = socket(PF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (sk < 0) {
        perror("Can't create socket");
        return -1;
    }

    port = atoi(argv[2]);

    memset(&addr, 0, sizeof(addr));
    addr.sin_family = AF_INET;

    if ((inet_aton(argv[1], &addr.sin_addr)) < 0) {
        perror("Can't resolve server address");
        return -1;
    }

    addr.sin_port = htons(port);
    if ((connect(sk, (struct sockaddr *)&addr, sizeof(addr))) < 0) {
        perror("Can't connect to server");
        return -1;
    }

    printf("Connected to %s:%d ...\n", argv[1], port);
    fflush(stdout);

	while (1) {
		if (read(sk, &rval, sizeof(rval)) > 0) {
			printf("Client <- Server: %d\n", rval);
			fflush(stdout);
		} else {
			fprintf(stderr, "CLIENT_ERROR: Failed to read from socket.\n");
            fflush(stderr);
			perror("read");
			break;
		}
	}
	close(sk);
	return -1;
}


int main(int argc, char **argv)
{
    if (argc != 3) {
        printf("Usage: %s <address> <port>\nExample: %s 127.0.0.1 8080\n", argv[0], argv[0]);
        return -1;
    }

    return main_cl(argc, argv);
}
