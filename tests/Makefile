# Variables #
PROJECT_NAME := criu-coordinator
BIN_PATH := ../target/debug/$(PROJECT_NAME)
TEST_NAMESPACE := default
MINIKUBE_IP := $(shell minikube ip)
CERT_PATH := ~/.minikube/profiles/minikube/client.crt
KEY_PATH := ~/.minikube/profiles/minikube/client.key

all: loop-1 loop-2 loop-3

loop-1:
	gcc loop.c -o $@
loop-2:
	gcc loop.c -o $@
loop-3:
	gcc loop.c -o $@

clean:
	rm -f loop-1 loop-2 loop-3
.PHONY: clean

test-k8s: deploy test-discovery

# DEPLOYMENT #
deploy:
	@echo "Deploying test pods..."
	-kubectl delete -f ./k8s/test-pod.yml 2>/dev/null || true
	kubectl apply -f ./k8s/test-pod.yml --validate=false
	@echo "Waiting for pods to be ready..."
	kubectl wait --for=condition=Ready pod/checkpoint-test-pod --timeout=60s
	kubectl wait --for=condition=Ready pod/checkpoint-test-db --timeout=60s
	@echo "Test pods deployed successfully."

# TEST #
# Test discovery functionality
test-discovery:
	@echo "Testing pod discovery..."
	$(BIN_PATH) kubernetes discover --namespace $(TEST_NAMESPACE) --selector app=checkpoint-test

# Test checkpoint functionality on a single container
test-checkpoint:
	@echo "Testing checkpoint functionality..."
	@echo "cert_path: $(CERT_PATH)"
	@echo "key_path: $(KEY_PATH)"
	@NODE_NAME=$$(kubectl get pod checkpoint-test-pod -o jsonpath='{.spec.nodeName}'); \
	echo "Pod is running on node: $$NODE_NAME"; \
	$(BIN_PATH) kubernetes checkpoint \
		--namespace $(TEST_NAMESPACE) \
		--pod checkpoint-test-pod \
		--container nginx \
		--node $$NODE_NAME \
		--cert-path $(CERT_PATH) \
		--key-path $(KEY_PATH)
	@echo "Checking for checkpoint files..."
	@kubectl get pods -o wide | grep checkpoint-test-pod
	@minikube ssh "ls -la /var/lib/kubelet/checkpoints/ | grep checkpoint-test-pod || echo 'No checkpoint found'"

# Test coordinated checkpoint functionality
test-coordinated-checkpoint:
	@echo "Testing coordinated checkpoint..."
	$(BIN_PATH) kubernetes coordinatedcheckpoint \
		--namespace $(TEST_NAMESPACE) \
		--app-name checkpoint-test \
		--selector app=checkpoint-test \
		--deps-file dependencies.json \
		--cert $(CERT_PATH) \
		--key $(KEY_PATH)
	@echo "Checking for checkpoint files..."
	@minikube ssh "ls -la /var/lib/kubelet/checkpoints/ | grep checkpoint-test || echo 'No checkpoint found'"

# CLEANUP #
cleanup:
	@echo "Cleaning up test resources..."
	-kubectl delete -f ./k8s/test-pod.yml
	@echo "Test resources cleaned up."
