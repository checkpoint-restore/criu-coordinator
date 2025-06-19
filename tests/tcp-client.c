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
        return -1;
    }

    port = atoi(argv[2]);

    memset(&addr, 0, sizeof(addr));
    addr.sin_family = AF_INET;

    if ((inet_aton(argv[1], &addr.sin_addr)) < 0) {
        return -1;
    }

    addr.sin_port = htons(port);
    if ((connect(sk, (struct sockaddr *)&addr, sizeof(addr))) < 0) {
        return -1;
    }

    printf("Connected to %s:%d ...\n", argv[1], port);

    while (1) {
        gettimeofday(&t0, NULL);
        while (read(sk, &rval, sizeof(rval)) == 0)
            sleep(0.0001);
        gettimeofday(&t1, NULL);
        printf("%f ms\n", (float)((t1.tv_sec - t0.tv_sec) * 1000.0 + (t1.tv_usec - t0.tv_usec) / 1000.0));
    }
    return -1;
}


int main(int argc, char **argv)
{
    if (argc == 3)
        main_cl(argc, argv);
    else
        printf("Usage: %s <address> <port>\nExample: %s 127.0.0.1 8080\n",
                argv[0], argv[0]);

    return 0;
}
