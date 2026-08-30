; CALL/RET and SP-relative addressing example.
;
; ADD_FUNC(a, b) returns a + b in AC.
; Arguments are pushed right-to-left and removed by the caller.

ORG 0x0010
START:
        LDA B
        PUSH
        LDA A
        PUSH

        CALL ADD_FUNC
        ADJSP 2

        STA RESULT
        HLT

ADD_FUNC:
        ADJSP -1

        ; SP+0 = local result
        ; SP+1 = return address
        ; SP+2 = argument a
        ; SP+3 = argument b
        LDSP 2
        ADDSP 3
        STSP 0

        LDSP 0
        ADJSP 1
        RET

A:      DEC 7
B:      DEC 35
RESULT: DEC 0
END
