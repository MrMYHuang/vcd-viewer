`timescale 1ns / 1ps

module tb_adder;

    localparam int WIDTH = 32;

    logic             clk;
    logic             rst_n;
    logic             valid_i;
    logic [WIDTH-1:0] a_i;
    logic [WIDTH-1:0] b_i;
    logic             cin_i;
    logic             valid_o;
    logic [WIDTH-1:0] sum_o;
    logic             cout_o;

    // Also include an IEEE 754 FP32 test bus to showcase FP32 formatting in the viewer
    logic [31:0]      fp32_probe;

    adder #(
        .WIDTH(WIDTH)
    ) u_dut (
        .clk     (clk),
        .rst_n   (rst_n),
        .valid_i (valid_i),
        .a_i     (a_i),
        .b_i     (b_i),
        .cin_i   (cin_i),
        .valid_o (valid_o),
        .sum_o   (sum_o),
        .cout_o  (cout_o)
    );

    // 100 MHz clock (10 ns period)
    initial begin
        clk = 1'b0;
        forever #5 clk = ~clk;
    end

    initial begin
        $dumpfile("adder.vcd");
        $dumpvars(0, tb_adder);

        // Initialize signals
        rst_n      = 1'b0;
        valid_i    = 1'b0;
        a_i        = '0;
        b_i        = '0;
        cin_i      = 1'b0;
        fp32_probe = 32'h0000_0000; // 0.0

        // Release reset after 25 ns
        #25;
        rst_n = 1'b1;

        // Cycle 1: 15 + 27 + 0 = 42
        @(posedge clk);
        valid_i    = 1'b1;
        a_i        = 32'd15;
        b_i        = 32'd27;
        cin_i      = 1'b0;
        fp32_probe = 32'h3F80_0000; // 1.0

        // Cycle 2: 0x0000_1000 + 0x0000_2000 + 1 = 0x0000_3001
        @(posedge clk);
        valid_i    = 1'b1;
        a_i        = 32'h0000_1000;
        b_i        = 32'h0000_2000;
        cin_i      = 1'b1;
        fp32_probe = 32'h4049_0FDB; // 3.1415927 (pi)

        // Cycle 3: Unsigned overflow / carry out test: 0xFFFF_FFFF + 1 = 0x0000_0000 (cout = 1)
        @(posedge clk);
        valid_i    = 1'b1;
        a_i        = 32'hFFFF_FFFF;
        b_i        = 32'h0000_0001;
        cin_i      = 1'b0;
        fp32_probe = 32'hC020_0000; // -2.5

        // Cycle 4: Signed negative + positive: -100 + 250 = 150
        @(posedge clk);
        valid_i    = 1'b1;
        a_i        = -32'sd100;
        b_i        = 32'sd250;
        cin_i      = 1'b0;
        fp32_probe = 32'h4228_0000; // 42.0

        // Cycle 5: Signed negative + negative: -500 + (-300) = -800
        @(posedge clk);
        valid_i    = 1'b1;
        a_i        = -32'sd500;
        b_i        = -32'sd300;
        cin_i      = 1'b0;
        fp32_probe = 32'h7F80_0000; // +Inf

        // Cycle 6..15: Sweep ramp
        for (int i = 1; i <= 10; i++) begin
            @(posedge clk);
            valid_i    = 1'b1;
            a_i        = i * 100;
            b_i        = i * 50;
            cin_i      = i[0];
            fp32_probe = $shortrealtobits(shortreal'(i) * 1.25);
        end

        @(posedge clk);
        valid_i = 1'b0;
        #30;

        if (valid_o !== 1'b0 && sum_o === 32'h0 && cout_o === 1'b1) begin
            $display("Final state check");
        end
        $display("Simulation complete. VCD dumped to example/adder.vcd");
        $finish;
    end

endmodule
