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

NAME = criu-coordinator

.PHONY: all
all: $(NAME)  ## Build criu-coordinator binary

BASHINSTALLDIR=${PREFIX}/share/bash-completion/completions
ZSHINSTALLDIR=${PREFIX}/share/zsh/site-functions
FISHINSTALLDIR=${PREFIX}/share/fish/vendor_completions.d

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

target/$(BUILD)/$(NAME): $(DEPS)
	$(CARGO) build $(BUILD_FLAGS)

$(NAME): target/$(BUILD)/$(NAME)
	cp -a $< $@

.PHONY: install
install: target/$(BUILD)/$(NAME) install.completions  ## Install binary and completions
	@echo "  INSTALL " $<
	@mkdir -p $(DESTDIR)$(BINDIR)
	@install -m0755 $< $(BINDIR)/$(NAME)

.PHONY: uninstall
uninstall: uninstall.completions  ## Uninstall binary and completions
	@echo " UNINSTALL" $(NAME)
	$(RM) $(addprefix $(DESTDIR)$(BINDIR)/,$(NAME))

.PHONY: lint
lint:  ## Run clippy lint checks
	$(CARGO) clippy --all-targets --all-features -- -D warnings

.PHONY: lint-fix
lint-fix:  ## Automatically fix lint issues
	$(CARGO) clippy --fix --all-targets --all-features -- -D warnings

.PHONY: test
test:  ## Run tests
	$(CARGO) test

.PHONY: completions
completions: $(NAME)  ## Generate shell completions
	declare -A outfiles=([bash]=%s [zsh]=_%s [fish]=%s.fish);\
	for shell in $${!outfiles[*]}; do \
		outfile=$$(printf "completions/$$shell/$${outfiles[$$shell]}" $(NAME)); \
		./$(NAME) completions $$shell >| $$outfile; \
	done

.PHONY: validate.completions
validate.completions: SHELL:=/usr/bin/env bash # Set shell to bash for this target
validate.completions:  ## Validate generated completions with their shells
	# Check if the files can be loaded by the shell
	. completions/bash/$(NAME)
	if [ -x /bin/zsh ]; then \
		/bin/zsh -c 'autoload -Uz compinit; compinit; source completions/zsh/_$(NAME)'; \
	fi
	if [ -x /bin/fish ]; then /bin/fish completions/fish/$(NAME).fish; fi

.PHONY: install.completions
install.completions:  ## Install generated completions
	@install -d -m 755 ${DESTDIR}${BASHINSTALLDIR}
	@install -m 644 completions/bash/$(NAME) ${DESTDIR}${BASHINSTALLDIR}
	@install -d -m 755 ${DESTDIR}${ZSHINSTALLDIR}
	@install -m 644 completions/zsh/_$(NAME) ${DESTDIR}${ZSHINSTALLDIR}
	@install -d -m 755 ${DESTDIR}${FISHINSTALLDIR}
	@install -m 644 completions/fish/$(NAME).fish ${DESTDIR}${FISHINSTALLDIR}

.PHONY: uninstall.completions
uninstall.completions:  ## Remove installed completions
	@$(RM) $(addprefix ${DESTDIR}${BASHINSTALLDIR}/,$(NAME))
	@$(RM) $(addprefix ${DESTDIR}${ZSHINSTALLDIR}/,_$(NAME))
	@$(RM) $(addprefix ${DESTDIR}${FISHINSTALLDIR}/,$(NAME).fish)

.PHONY: clean
clean:  ## Clean build artifacts
	rm -rf target $(NAME)

.PHONY: help
help:  ## Show this help message
	@awk 'BEGIN {FS = ":.*##"; printf "\nUsage:\n  make <target>\n\nTargets:\n"} \
		/^[a-zA-Z0-9_.-]+:.*##/ { printf " * \033[36m%s\033[0m -%s\n", $$1, $$2 }' $(MAKEFILE_LIST)
