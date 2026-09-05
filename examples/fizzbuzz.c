/*
 * FizzBuzz for EX3 C v0.1.
 *
 * This subset has no strings, so words are written one character at a time.
 * Decimal conversion demonstrates recursion as well as division and modulo.
 *
 * Build and run:
 *   ex3 cc examples/fizzbuzz.c
 *   ex3 run examples/fizzbuzz.mem --io legacy --max-steps 1000000
 */

void putchar(int c);

void print_number(int value)
{
    if (value >= 10) {
        print_number(value / 10);
    }
    putchar(48 + value % 10);
}

void print_fizz(void)
{
    putchar(70); /* F */
    putchar(105); /* i */
    putchar(122); /* z */
    putchar(122); /* z */
}

void print_buzz(void)
{
    putchar(66); /* B */
    putchar(117); /* u */
    putchar(122); /* z */
    putchar(122); /* z */
}

int main(void)
{
    int value;

    value = 1;
    while (value <= 100) {
        if (value % 15 == 0) {
            print_fizz();
            print_buzz();
        } else if (value % 3 == 0) {
            print_fizz();
        } else if (value % 5 == 0) {
            print_buzz();
        } else {
            print_number(value);
        }

        putchar(10); /* newline */
        value = value + 1;
    }

    return 0;
}
