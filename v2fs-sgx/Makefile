export SGX_SDK ?= /opt/sgxsdk
export SGX_MODE ?= HW

.PHONY: default
default: build-release

.PHONY: build
build:
	$(MAKE) -C enclave DEBUG=1
	cargo build

build-release:
	$(MAKE) -C enclave
	cargo build --release

.PHONY: clean
clean:
	-rm -rf target
	-$(MAKE) -C enclave clean