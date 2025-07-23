#!/bin/bash

# Build script for sammy_monitor Docker image

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default values
IMAGE_NAME="sammy_monitor"
PUSH=false
TAG="latest"

usage() {
    echo "Usage: $0 [OPTIONS]"
    echo "Options:"
    echo "  -p, --push      Push image to registry"
    echo "  -t, --tag TAG   Tag for the image (default: latest)"
    echo "  -n, --name NAME Image name (default: sammy_monitor)"
    echo "  -h, --help      Show this help message"
    exit 1
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -p|--push)
            PUSH=true
            shift
            ;;
        -t|--tag)
            TAG="$2"
            shift 2
            ;;
        -n|--name)
            IMAGE_NAME="$2"
            shift 2
            ;;
        -h|--help)
            usage
            ;;
        *)
            echo "Unknown option $1"
            usage
            ;;
    esac
done

echo -e "${BLUE}Building Docker image for sammy_monitor (Alpine-based)${NC}"

echo -e "${GREEN}Building image...${NC}"
docker build -f Dockerfile -t "${IMAGE_NAME}:${TAG}" -t "${IMAGE_NAME}:latest" .

if [[ "$PUSH" == true ]]; then
    echo -e "${GREEN}Pushing images...${NC}"
    docker push "${IMAGE_NAME}:${TAG}"
    if [[ "$TAG" != "latest" ]]; then
        docker push "${IMAGE_NAME}:latest"
    fi
fi

echo -e "${GREEN}Build complete!${NC}"

# Show built images
echo -e "${BLUE}Built images:${NC}"
docker images | grep "${IMAGE_NAME}" | head -5
