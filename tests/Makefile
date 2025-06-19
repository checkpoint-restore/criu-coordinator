all: loop-1 loop-2 loop-3 tcp-client tcp-server

loop-1:
	gcc loop.c -o $@
loop-2:
	gcc loop.c -o $@
loop-3:
	gcc loop.c -o $@
tcp-client:
	gcc tcp-client.c -o $@
tcp-server:
	gcc tcp-server.c -o $@

clean:
	rm -f loop-1 loop-2 loop-3 tcp-client tcp-server
.PHONY: clean
