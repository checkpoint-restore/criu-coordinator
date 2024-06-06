# Copyright (c) 2023 University of Oxford.
# Copyright (c) 2023 Red Hat, Inc.
# All rights reserved.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

all: criu-coordinator

PREFIX ?= $(DESTDIR)/usr/local
BINDIR ?= $(PREFIX)/bin

BUILD ?= release

BUILD_FLAGS=

ifeq ($(BUILD),release)
	BUILD_FLAGS+=--release
endif

DEPS = $(wildcard src/*.rs src/**/*.rs) Cargo.toml

CARGO=$(HOME)/.cargo/bin/cargo
ifeq (,$(wildcard $(CARGO)))
	CARGO=cargo
endif

target/$(BUILD)/criu-coordinator: $(DEPS)
	$(CARGO) build $(BUILD_FLAGS)

criu-coordinator: target/$(BUILD)/criu-coordinator
	cp -a $< $@

install: target/$(BUILD)/criu-coordinator
	install -m0755 $< $(BINDIR)/criu-coordinator

uninstall:
	$(RM) $(addprefix $(BINDIR)/,criu-coordinator)

lint:
	$(CARGO) clippy --all-targets --all-features -- -D warnings

lint-fix:
	$(CARGO) clippy --fix --all-targets --all-features -- -D warnings

test:
	$(CARGO) test

clean:
	rm -rf target criu-coordinator target

.PHONY: all clean install uninstall lint lint-fix test
