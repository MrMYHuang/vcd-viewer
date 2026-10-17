VERILATOR ?= verilator
VFLAGS = --binary --trace -Wall -Wno-fatal --top-module tb_adder

.PHONY: all run clean view

all: adder.vcd

adder.vcd: adder.sv tb_adder.sv
	$(VERILATOR) $(VFLAGS) adder.sv tb_adder.sv -o Vtb_adder
	./obj_dir/Vtb_adder

run: adder.vcd

view: adder.vcd
	cargo run --release --manifest-path ../Cargo.toml -- adder.vcd

clean:
	rm -rf obj_dir adder.vcd
