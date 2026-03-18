.PHONY: ffi go-example test-go test clean license-3rdparty

ffi:
	cargo build -p barkus-ffi --release

go-example: ffi
	go build -o target/release/barkus-gen ./go/cmd/barkus-gen

test-go: ffi
	go test ./go/pkg/barkus/...

test: test-go
	cargo test --workspace

license-3rdparty:
	dd-license-attribution generate-sbom-csv \
		--only-transitive-dependencies \
		https://github.com/DataDog/barkus > LICENSE-3rdparty.csv

clean:
	cargo clean
	rm -f target/release/barkus-gen
