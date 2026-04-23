base_image := "space-jam-base"
code_image := "space-jam-dev"

# Build both base and claude code images
build: build-base build-code

# Build the base image (OS + Rust toolchain)
build-base:
    docker build -t {{base_image}} .

# Build the claude code image (on top of base)
build-code:
    docker build -t {{code_image}} -f Dockerfile.claude --build-arg BASE_IMAGE={{base_image}} .

# Enter the container running claude code
code: build-code
    docker run --rm -it \
        -v "$PWD":/workspace \
        -v space-jam-home:/home/node \
        {{code_image}}

