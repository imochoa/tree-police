FROM scratch
ARG TARGETARCH
COPY dist/${TARGETARCH}/tree-police /tree-police
ENTRYPOINT ["/tree-police"]
