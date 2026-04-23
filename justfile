base_image := "space-jam-base"
code_image := "space-jam-dev"
ocode_image := "space-jam-ocode"

# Build both base and claude code images
build: build-base build-code build-ocode

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

# Build the opencode image (on top of base)
build-ocode:
    docker build -t {{ocode_image}} -f Dockerfile.opencode --build-arg BASE_IMAGE={{base_image}} .

# Enter the container running opencode
ocode: build-ocode
    docker run --rm -it \
        -v "$PWD":/workspace \
        -v space-jam-home:/home/node \
        {{ocode_image}}

