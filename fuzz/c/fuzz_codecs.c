/* Fuzzes the decoding of values coming from Rust, in the code generated for the
 * `tour` fixture: whatever the bytes, reading them either fails cleanly
 * (`reader.failed`) or yields a value that writes back to exactly the bytes it
 * was read from; nothing reads out of bounds, overflows, leaks or allocates
 * without bound.
 *
 * Built by scripts/ci/fuzz_c.nu, with libFuzzer when clang has it, else with
 * -DUNIFFI_FUZZ_SMOKE and the pseudo-random driver in main().
 */
#include "tour.c"

#define FUZZ_TYPE(read, write, free_stmt, ctype)                                   \
    do {                                                                          \
        uniffi_reader r = uniffi_reader_new(rest, n);                             \
        ctype value = read(&r);                                                   \
        if (!r.failed) {                                                          \
            uniffi_writer w = {NULL, 0, 0};                                       \
            write(&w, value);                                                     \
            if (w.len != r.pos || (w.len && memcmp(w.data, rest, w.len) != 0)) {  \
                fprintf(stderr, "round trip changed the bytes of " #ctype "\n");  \
                abort();                                                          \
            }                                                                     \
            uniffi_writer_free(&w);                                               \
        }                                                                         \
        free_stmt;                                                                \
    } while (0)

int LLVMFuzzerTestOneInput(const uint8_t *data, size_t size);

int LLVMFuzzerTestOneInput(const uint8_t *data, size_t size) {
    if (size == 0) {
        return 0;
    }
    /* The first byte picks the type, the rest is the buffer. */
    const uint8_t *rest = data + 1;
    size_t n = size - 1;
    switch (data[0] % 10) {
        case 0:
            FUZZ_TYPE(uniffi_read_string, uniffi_write_string, uniffi_string_free(&value), uniffi_string);
            break;
        case 1:
            FUZZ_TYPE(uniffi_read_bytes, uniffi_write_bytes, uniffi_bytes_free(&value), uniffi_bytes);
            break;
        case 2:
            FUZZ_TYPE(tour_uniffi_read_SequenceOptionSequenceString, tour_uniffi_write_SequenceOptionSequenceString,
                      tour_SequenceOptionSequenceString_free(&value), tour_SequenceOptionSequenceString);
            break;
        case 3:
            FUZZ_TYPE(tour_uniffi_read_MapStringSequenceU64, tour_uniffi_write_MapStringSequenceU64,
                      tour_MapStringSequenceU64_free(&value), tour_MapStringSequenceU64);
            break;
        case 4:
            FUZZ_TYPE(uniffi_read_timestamp, uniffi_write_timestamp, (void)value, uniffi_timestamp);
            break;
        case 5:
            FUZZ_TYPE(uniffi_read_duration, uniffi_write_duration, (void)value, uniffi_duration);
            break;
        case 6:
            FUZZ_TYPE(tour_uniffi_read_Settings, tour_uniffi_write_Settings, tour_Settings_free(&value), tour_Settings);
            break;
        case 7:
            FUZZ_TYPE(tour_uniffi_read_Shape, tour_uniffi_write_Shape, tour_Shape_free(&value), tour_Shape);
            break;
        case 8:
            FUZZ_TYPE(tour_uniffi_read_Expr, tour_uniffi_write_Expr, tour_Expr_free(&value), tour_Expr);
            break;
        default:
            FUZZ_TYPE(tour_uniffi_read_TreeNode, tour_uniffi_write_TreeNode, tour_TreeNode_free(&value), tour_TreeNode);
            break;
    }
    return 0;
}

#ifdef UNIFFI_FUZZ_SMOKE
int main(int argc, char **argv) {
    unsigned long runs = argc > 1 ? strtoul(argv[1], NULL, 10) : 200000;
    uint64_t state = 42;
    uint8_t input[64];
    for (unsigned long i = 0; i < runs; ++i) {
        state = state * 6364136223846793005ull + 1442695040888963407ull;
        size_t len = (size_t)(state >> 58);
        for (size_t j = 0; j < len; ++j) {
            state = state * 6364136223846793005ull + 1442695040888963407ull;
            uint8_t byte = (uint8_t)(state >> 33);
            /* Mostly small values, so lengths and tags are often plausible. */
            input[j] = (state >> 61) == 0 ? byte : (uint8_t)(byte % 4);
        }
        LLVMFuzzerTestOneInput(input, len);
    }
    printf("%lu inputs, no crash\n", runs);
    return 0;
}
#endif
