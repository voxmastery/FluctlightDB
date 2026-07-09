.PHONY: reproduce-locomo reproduce-locomo-source reproduce-beam-smoke test-native-wheel

reproduce-locomo:
	bash scripts/reproduce-locomo.sh

reproduce-locomo-source:
	REPRODUCE_FROM_SOURCE=1 bash scripts/reproduce-locomo.sh

reproduce-beam-smoke:
	bash scripts/reproduce-beam-smoke.sh

test-native-wheel:
	bash scripts/verify-pypi-wheel.sh
