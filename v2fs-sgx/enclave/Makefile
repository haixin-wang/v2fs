######## SGX SDK Settings ########

SGX_ARCH ?= x64
DEBUG ?= 0
CC ?= gcc

RUST_SDK_PATH := ../rust-sgx-sdk
RUST_BUILD_FLAGS :=
SGX_COMMON_CFLAGS := -m64
SGX_LIBRARY_PATH := $(SGX_SDK)/lib64
SGX_ENCLAVE_SIGNER := $(SGX_SDK)/bin/x64/sgx_sign
SGX_EDGER8R := $(SGX_SDK)/bin/x64/sgx_edger8r

ifeq ($(SGX_DEBUG), 1)
	SGX_COMMON_CFLAGS += -O0 -g
	TARGET_DIR := target/debug
	OUTPUT_DIR := ../target/debug
else
	RUST_BUILD_FLAGS += --release
	SGX_COMMON_CFLAGS += -O2
	TARGET_DIR := target/release
	OUTPUT_DIR := ../target/release
endif

ifneq ($(SGX_MODE), HW)
	ENCLAVE_NAME := vsqlite_enclave_sim
	Trts_Library_Name := sgx_trts_sim
	Service_Library_Name := sgx_tservice_sim
else
	ENCLAVE_NAME := vsqlite_enclave
	Trts_Library_Name := sgx_trts
	Service_Library_Name := sgx_tservice
endif

Crypto_Library_Name := sgx_tcrypto

ENCLAVE_CONFIG := Enclave.config.xml
ENCLAVE_EDL := Enclave.edl
ENCLAVE_KEY := Enclave_private.pem
ENCLAVE_LDS := Enclave.lds

ENCLAVE_FILE := $(TARGET_DIR)/lib$(ENCLAVE_NAME).so
ENCLAVE_SIGNED_FILE := $(OUTPUT_DIR)/lib$(ENCLAVE_NAME).signed.so

ENCLAVE_TRUST_OBJ_NAME := Enclave_t.o
ENCLAVE_TRUST_OBJ_FILE := $(TARGET_DIR)/$(ENCLAVE_TRUST_OBJ_NAME)

RUST_LIB_NAME := vsqlite_enclave_rust
RUST_LIB_FILE := $(TARGET_DIR)/lib$(RUST_LIB_NAME).a

SGX_ENCLAVE_CFLAGS := \
	$(SGX_COMMON_CFLAGS) -nostdinc -fvisibility=hidden -fpie -fstack-protector \
	-I$(RUST_SDK_PATH)/common/inc -I$(RUST_SDK_PATH)/edl -I$(SGX_SDK)/include \
	-I$(SGX_SDK)/include/tlibc -I$(SGX_SDK)/include/stlport \
	-I$(SGX_SDK)/include/epid -I$(TARGET_DIR)

SGX_ENCLAVE_LDFLAGS := \
	$(SGX_COMMON_CFLAGS) -Wl,--no-undefined -nostdlib -nodefaultlibs -nostartfiles \
	-L$(SGX_LIBRARY_PATH) -Wl,--whole-archive -l$(Trts_Library_Name) -Wl,--no-whole-archive \
	-Wl,--start-group -lsgx_tstdc -l$(Service_Library_Name) -l$(Crypto_Library_Name) \
	-L$(TARGET_DIR) -l$(RUST_LIB_NAME) \
	-Wl,--end-group \
	-Wl,-Bstatic -Wl,-Bsymbolic -Wl,--no-undefined \
	-Wl,-pie,-eenclave_entry -Wl,--export-dynamic \
	-Wl,--defsym,__ImageBase=0 \
	-Wl,--gc-sections \
	-Wl,--version-script=$(ENCLAVE_LDS)

.PHONY: default
default: enclave_lib 

######## enclave_lib generation ########

.PHONY: enclave_lib
enclave_lib: $(ENCLAVE_SIGNED_FILE)

$(ENCLAVE_SIGNED_FILE): $(ENCLAVE_CONFIG) $(ENCLAVE_KEY) $(ENCLAVE_FILE)
	@echo "SIGN =>  $@"
	@mkdir -p $(OUTPUT_DIR)
	$(SGX_ENCLAVE_SIGNER) sign -key $(ENCLAVE_KEY) -enclave $(ENCLAVE_FILE) -out $@ -config $(ENCLAVE_CONFIG)
	
$(ENCLAVE_FILE): $(ENCLAVE_TRUST_OBJ_FILE) $(RUST_LIB_FILE)
	@echo "LINK =>  $@"
	$(CXX) $< -o $@ $(SGX_ENCLAVE_LDFLAGS)
	

######## Enclave_t.o generation ########

ENCLAVE_TRUST_SRC := \
	$(TARGET_DIR)/Enclave_t.c \
	$(TARGET_DIR)/Enclave_t.h 

.PHONY: enclave_trust_obj
enclave_trust_obj: $(ENCLAVE_TRUST_OBJ_FILE)

$(ENCLAVE_TRUST_OBJ_FILE): $(ENCLAVE_TRUST_SRC)
	@echo "CC   <=  $<"
	$(CC) $(SGX_ENCLAVE_CFLAGS) -c $< -o $@
	

$(ENCLAVE_TRUST_SRC): $(ENCLAVE_EDL)
	@echo "GEN  =>  $@"
	@mkdir -p $(TARGET_DIR)
	$(SGX_EDGER8R) --trusted $(ENCLAVE_EDL) \
		--search-path $(SGX_SDK)/include --search-path $(RUST_SDK_PATH)/edl \
		--trusted-dir $(TARGET_DIR)
	

######## rust_lib generation ########

RUST_SRC := \
	$(shell find \
		src \
		../rusqlite/src \
		-type f -name "*.rs") \
	$(shell find \
		. \
		../rusqlite \
		-type f -name "Cargo.toml" -o -name "Cargo.lock")

.PHONY: rust_lib
rust_lib: $(RUST_LIB_FILE)

$(RUST_LIB_FILE): $(RUST_SRC)
	@echo "BUILD  =>  $(RUST_LIB_FILE)"
	cargo build $(RUST_BUILD_FLAGS)
	

######## clean ########

.PHONY: clean
clean:
	@rm -rf target
	@rm -rf ../target/*/lib$(ENCLAVE_NAME)*.signed.so