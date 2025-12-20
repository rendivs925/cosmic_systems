# Makefile for Cosmic Frontier Simulator

.DEFAULT_GOAL := help

.PHONY: help space-simulation gyro-propulsion clean

help:
	@echo "Available targets:"
	@echo "  space-simulation Run the main space simulation"
	@echo "  gyro-propulsion  Run the gyroscopic propulsion simulation"
	@echo "  clean            Clean build artifacts"

space-simulation:
	cargo run --release

gyro-propulsion:
	cargo run --release gyro

clean:
	cargo clean
