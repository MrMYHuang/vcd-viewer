`timescale 1ns / 1ps

module adder #(
    parameter int WIDTH = 32
) (
    input  logic                 clk,
    input  logic                 rst_n,
    input  logic                 valid_i,
    input  logic [WIDTH-1:0]     a_i,
    input  logic [WIDTH-1:0]     b_i,
    input  logic                 cin_i,
    output logic                 valid_o,
    output logic [WIDTH-1:0]     sum_o,
    output logic                 cout_o
);

    logic [WIDTH:0] full_sum;

    assign full_sum = {1'b0, a_i} + {1'b0, b_i} + WIDTH'(cin_i);

    always_ff @(posedge clk or negedge rst_n) begin
        if (!rst_n) begin
            valid_o <= 1'b0;
            sum_o   <= '0;
            cout_o  <= 1'b0;
        end else begin
            valid_o <= valid_i;
            if (valid_i) begin
                sum_o  <= full_sum[WIDTH-1:0];
                cout_o <= full_sum[WIDTH];
            end
        end
    end

endmodule
