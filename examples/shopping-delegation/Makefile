PYTHON ?= python3
ROOT := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
REPO := $(abspath $(ROOT)/../..)
export PYTHONPATH := $(REPO)/sdks/python$(if $(PYTHONPATH),:$(PYTHONPATH),)

.PHONY: demo bench generate selftest check

demo:
	cd $(ROOT) && $(PYTHON) -m shopping_delegation demo

bench:
	cd $(ROOT) && $(PYTHON) -m shopping_delegation bench

generate:
	cd $(ROOT) && $(PYTHON) -m shopping_delegation generate

selftest:
	cd $(ROOT) && $(PYTHON) -m shopping_delegation selftest

check: selftest generate
	cd $(ROOT) && $(PYTHON) -m shopping_delegation generate --out /tmp/shopping-sessions.jsonl
	cmp $(ROOT)/data/sessions.jsonl /tmp/shopping-sessions.jsonl
