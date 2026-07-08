.PHONY: reproduce-locomo reproduce-locomo-source test-native-wheel

reproduce-locomo:
	bash scripts/reproduce-locomo.sh

reproduce-locomo-source:
	REPRODUCE_FROM_SOURCE=1 bash scripts/reproduce-locomo.sh

test-native-wheel:
	bash scripts/verify-pypi-wheel.sh
