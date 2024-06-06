CRIU Coordinator
====================

_criu-coordinator_ enables checkpoint coordination among multiple processes, containers, or Kubernetes Pods.

Usage Example
-------------

1. Start coordinator server

```console
criu-coordinator server --address 127.0.0.1 --port 8080
```

2. Create directory for CRIU image files and copy `criu-coordinator.json`

```console
mkdir /tmp/test
cp example-config/criu-coordinator.json /tmp/test/
```

3. Configure CRIU to use criu-coordinator

```console
mkdir -p /etc/criu/
echo action-script="$(which criu-coordinator)" | sudo tee /etc/criu/default.conf
```

License
-------

criu-coordinator is licensed under the
[Apache 2.0 license](https://www.apache.org/licenses/LICENSE-2.0).
