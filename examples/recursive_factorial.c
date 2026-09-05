/*
 * Function calls, recursion, parameters, local variables, and multiplication.
 *
 * Build and run:
 *   ex3 cc examples/recursive_factorial.c
 *   ex3 run examples/recursive_factorial.mem
 *
 * main returns factorial(6) = 720 (AC = 0x000002d0).
 */

int factorial(int n)
{
    if (n <= 1) {
        return 1;
    }

    return n * factorial(n - 1);
}

int main(void)
{
    int result;

    result = factorial(6);
    return result;
}
