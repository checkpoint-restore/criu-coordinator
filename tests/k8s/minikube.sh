#!/bin/bash
set -e

# Colors for better output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${YELLOW}Setting up Minikube with CRIU checkpoint support...${NC}"

# Check if minikube is already running
if minikube status &>/dev/null; then
    echo -e "${YELLOW}Stopping existing Minikube cluster...${NC}"
    minikube stop
fi

# Start Minikube with required feature gates
echo -e "${YELLOW}Starting Minikube with ContainerCheckpoint feature gate...${NC}"
minikube start \
    --container-runtime=cri-o \
    --feature-gates="ContainerCheckpoint=true" \
    --extra-config=kubelet.feature-gates="ContainerCheckpoint=true"

# Ensure CRI-O is set up with CRIU support
echo -e "${YELLOW}Configuring CRI-O for CRIU support...${NC}"
minikube ssh "sudo sed -i 's/.*enable_criu_support.*/enable_criu_support = true/' /etc/crio/crio.conf || echo 'enable_criu_support = true' | sudo tee -a /etc/crio/crio.conf"
minikube ssh "sudo systemctl restart crio"

# Verify CRIU is installed
echo -e "${YELLOW}Checking if CRIU is installed...${NC}"
if minikube ssh "which criu" &>/dev/null; then
    echo -e "${GREEN}CRIU is already installed.${NC}"
else
    echo -e "${YELLOW}Installing CRIU...${NC}"
    minikube ssh "sudo dnf install -y criu || sudo apt-get update && sudo apt-get install -y criu"
fi

# Create directory for checkpoints if it doesn't exist
echo -e "${YELLOW}Creating checkpoint directory...${NC}"
minikube ssh "sudo mkdir -p /var/lib/kubelet/checkpoints"

# Show certificate paths for reference
echo -e "${YELLOW}Certificate paths for kubelet API:${NC}"
CERT_PATH=$(minikube ssh "find /var/lib/minikube/certs -name 'client.crt' | head -1")
KEY_PATH=$(minikube ssh "find /var/lib/minikube/certs -name 'client.key' | head -1")
echo -e "${GREEN}Certificate path: ${CERT_PATH}${NC}"
echo -e "${GREEN}Key path: ${KEY_PATH}${NC}"

# Print a success message
echo -e "${GREEN}Minikube setup completed successfully!${NC}"
echo -e "${GREEN}Use these paths in your checkpoint commands:${NC}"
echo -e "  --cert ${CERT_PATH}"
echo -e "  --key ${KEY_PATH}"
echo -e "${YELLOW}Now you can run 'make deploy' to deploy the test pods.${NC}"