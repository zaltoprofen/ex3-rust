int total;

int classify(unsigned int value)
{
    switch (value & 3u) {
    case 0:
        return 10;
    case 1:
        return 20;
    default:
        return 30;
    }
}

int main(void)
{
    int index;

    index = 0;
    total = 0;
    while (index < 4) {
        index = index + 1;
        if (index == 2) {
            continue;
        }
        total = total + index;
    }
    if ((total != 8 && total != 9) || classify(2u) != 30) {
        goto failed;
    }
    return 42;

failed:
    return 1;
}
