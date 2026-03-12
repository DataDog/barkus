.PHONY: ffi go-example test-go test clean

ffi:
	cargo build -p barkus-ffi --release

go-example: ffi
	go build -o target/release/barkus-gen ./go/cmd/barkus-gen

test-go: ffi
	go test ./go/pkg/barkus/...

test: test-go
	cargo test --workspace

clean:
	cargo clean
	rm -f target/release/barkus-gen
