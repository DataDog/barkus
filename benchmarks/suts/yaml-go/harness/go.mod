module github.com/DataDog/barkus/benchmarks/suts/yaml-go/harness

go 1.22

toolchain go1.26.2

require (
	github.com/DataDog/barkus v0.0.0
	gopkg.in/yaml.v3 v3.0.1
)

replace github.com/DataDog/barkus => ../../../..
