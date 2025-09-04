CRIU Coordinator
====================

_criu-coordinator_ enables checkpoint coordination among multiple processes, containers, or Kubernetes Pods.


Installation
------------

To install _criu-coordinator_, follow these steps:

1. Clone the repository
```console
git clone https://github.com/checkpoint-restore/criu-coordinator.git
cd criu-coordinator
```

2. Build the project
```console
make
```

3. Install the binary
```console
make install
```

Usage
-------------

__criu-coordinator__ operates in a client-server model. The server manages the coordination of checkpoints, while clients are invoked as CRIU action scripts that communicate with the server during checkpoint and restore operations.

1. Start coordinator server

```console
criu-coordinator server --address 127.0.0.1 --port 8080
```

2. Create a configuration

Create a global configuration file at /etc/criu/criu-coordinator.json. For example:

```json
{
  "address": "127.0.0.1",
  "port": "8080",
  "dependencies": {
    "A": ["B"],
    "B": ["A"]
  }
}
```
A and B could be process names, process IDs, or container IDs. The dependencies define which processes must be checkpointed together.


Alternatively, for simple processes, you can create a per process configuration file `criu-coordinator.json` and place it in the directory where CRIU images will be stored. For example:

For process A:
```json
{
	"id": "A",
	"dependencies": ["B"],
	"address": "127.0.0.1",
	"port": "8080",
	"log-file": "coordinator.log"
}

```

For process B:
```json
{
    "id": "B",
    "dependencies": ["A"],
    "address": "127.0.0.1",
    "port": "8080",
    "log-file": "coordinator.log"
}
```

3. Configure CRIU to use criu-coordinator as the action script

```console
mkdir -p /etc/criu/
echo action-script="$(which criu-coordinator)" | sudo tee /etc/criu/default.conf
```

4. Coordinated Checkpoint/Restore

You can now perform coordinated checkpoint and restore operations. For example, to checkpoint process A and B:

In terminal 1 (for process A):

```console
sudo criu dump -t <PID_A> -D /tmp/images-a -j
```

In terminal 2 (for process B):

```console
sudo criu dump -t <PID_B> -D /tmp/images-b -j
```

To restore the processes, use the following commands:

In terminal 1 (for process A):
```console
sudo criu restore -D /tmp/images-a
```

In terminal 2 (for process B):
```console
sudo criu restore -D /tmp/images-b
```

License
-------

criu-coordinator is licensed under the
[Apache 2.0 license](https://www.apache.org/licenses/LICENSE-2.0).
