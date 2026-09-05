/*
 * Globals, signed/unsigned expressions, while, switch fall-through, break,
 * continue, goto, and short-circuit logical operators.
 *
 * Build and run:
 *   ex3 cc examples/language_features.c
 *   ex3 run examples/language_features.mem
 *
 * main returns 42 (AC = 0x0000002a).
 */

int total;
uint32_t mask = 0xffffffffU;

int classify(uint32_t value)
{
    switch (value & 3u) {
    case 0:
        return 10;
    case 1:
        return 20;
    case 2:
        return 30;
    default:
        return 40;
    }
}

int main(void)
{
    int i;

    i = 0;
    total = 0;
    while (i < 6) {
        i = i + 1;
        if (i == 2) {
            continue;
        }
        total = total + i;
    }

    /* The assignment on the right is skipped by short-circuit evaluation. */
    if (1 || (total = 0)) {
        total = total + classify(mask);
    }

    if (total != 59) {
        goto failed;
    }

    return 42;

failed:
    return 1;
}
