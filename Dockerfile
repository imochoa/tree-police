FROM scratch
COPY dist/tree-police /tree-police
ENTRYPOINT ["/tree-police"]
