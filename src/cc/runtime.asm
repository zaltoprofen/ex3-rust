__ex3_putchar:
SIO
__ex3_putchar_wait:
SKO
JMP __ex3_putchar_wait
LDSP 1
OUT
RET
__ex3_getchar:
SIO
__ex3_getchar_wait:
SKI
JMP __ex3_getchar_wait
CLA
INP
RET
__ex3_mul_u32:
ADJSP -4
CLA
STSP 0
LDSP 6
STSP 1
__ex3_mulu_loop:
LDSP 1
CMP 0
BEQ __ex3_mulu_end
LDA 1
STSP 2
LDSP 5
STSP 3
__ex3_mulu_inner:
LDSP 2
ADDSP 2
BUGE __ex3_mulu_take
CMPSP 1
BUGT __ex3_mulu_take
STSP 2
LDSP 3
ADDSP 3
STSP 3
JMP __ex3_mulu_inner
__ex3_mulu_take:
LDSP 1
SUBSP 2
STSP 1
LDSP 0
ADDSP 3
STSP 0
JMP __ex3_mulu_loop
__ex3_mulu_end:
LDSP 0
ADJSP 4
RET
__ex3_div_u32:
ADJSP -4
CLA
STSP 1
LDSP 5
STSP 0
__ex3_divu_loop:
LDSP 0
CMPSP 6
BULT __ex3_divu_end
LDSP 6
STSP 2
LDA 1
STSP 3
__ex3_divu_inner:
LDSP 2
ADDSP 2
BUGE __ex3_divu_take
CMPSP 0
BUGT __ex3_divu_take
STSP 2
LDSP 3
ADDSP 3
STSP 3
JMP __ex3_divu_inner
__ex3_divu_take:
LDSP 0
SUBSP 2
STSP 0
LDSP 0
LDSP 1
ADDSP 3
STSP 1
JMP __ex3_divu_loop
__ex3_divu_end:
LDSP 1
ADJSP 4
RET
__ex3_mod_u32:
ADJSP -2
LDSP 3
STSP 0
__ex3_modu_loop:
LDSP 0
CMPSP 4
BULT __ex3_modu_end
LDSP 4
STSP 1
__ex3_modu_inner:
LDSP 1
ADDSP 1
BUGE __ex3_modu_take
CMPSP 0
BUGT __ex3_modu_take
STSP 1
JMP __ex3_modu_inner
__ex3_modu_take:
LDSP 0
SUBSP 1
STSP 0
JMP __ex3_modu_loop
__ex3_modu_end:
LDSP 0
ADJSP 2
RET
__ex3_mul_i32:
ADJSP -3
LDSP 4
CMP 0
BGE __ex3_muli_aok
STSP 0
CLA
SUBSP 0
__ex3_muli_aok:
STSP 0
LDSP 5
CMP 0
BGE __ex3_muli_bok
STSP 1
CLA
SUBSP 1
__ex3_muli_bok:
STSP 1
LDSP 1
PUSH
LDSP 1
PUSH
CALL __ex3_mul_u32
ADJSP 2
STSP 2
LDSP 4
CMP 0
BLT __ex3_muli_aneg
LDSP 5
CMP 0
BLT __ex3_muli_neg
JMP __ex3_muli_ret
__ex3_muli_aneg:
LDSP 5
CMP 0
BLT __ex3_muli_ret
__ex3_muli_neg:
CLA
SUBSP 2
STSP 2
__ex3_muli_ret:
LDSP 2
ADJSP 3
RET
__ex3_div_i32:
ADJSP -3
LDSP 4
CMP 0
BGE __ex3_divi_aok
STSP 0
CLA
SUBSP 0
__ex3_divi_aok:
STSP 0
LDSP 5
CMP 0
BGE __ex3_divi_bok
STSP 1
CLA
SUBSP 1
__ex3_divi_bok:
STSP 1
LDSP 1
PUSH
LDSP 1
PUSH
CALL __ex3_div_u32
ADJSP 2
STSP 2
LDSP 4
CMP 0
BLT __ex3_divi_aneg
LDSP 5
CMP 0
BLT __ex3_divi_neg
JMP __ex3_divi_ret
__ex3_divi_aneg:
LDSP 5
CMP 0
BLT __ex3_divi_ret
__ex3_divi_neg:
CLA
SUBSP 2
STSP 2
__ex3_divi_ret:
LDSP 2
ADJSP 3
RET
__ex3_mod_i32:
ADJSP -2
LDSP 3
CMP 0
BGE __ex3_modi_aok
STSP 0
CLA
SUBSP 0
__ex3_modi_aok:
STSP 0
LDSP 4
CMP 0
BGE __ex3_modi_bok
STSP 1
CLA
SUBSP 1
__ex3_modi_bok:
STSP 1
LDSP 1
PUSH
LDSP 1
PUSH
CALL __ex3_mod_u32
ADJSP 2
STSP 0
LDSP 3
CMP 0
BGE __ex3_modi_ret
CLA
SUBSP 0
STSP 0
__ex3_modi_ret:
LDSP 0
ADJSP 2
RET
