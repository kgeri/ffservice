FROM mcr.microsoft.com/devcontainers/rust:0-1-bullseye

RUN apt-get update && export DEBIAN_FRONTEND=noninteractive \
    && apt-get -y install --no-install-recommends \
    clang \
    pkg-config \
    libavcodec-dev libavdevice-dev libavfilter-dev libavformat-dev libavutil-dev \
    protobuf-compiler libprotobuf-dev
