/* End-to-end test of the generated C bindings against the `tour` fixture. */
#include <stdio.h>
#include <string.h>

#include "tour.h"

static int failures = 0;
static int checks = 0;

#define CHECK(cond)                                                                    \
    do {                                                                               \
        ++checks;                                                                      \
        if (!(cond)) {                                                                 \
            ++failures;                                                                \
            fprintf(stderr, "%s:%d: CHECK failed: %s\n", __FILE__, __LINE__, #cond);   \
        }                                                                              \
    } while (0)

/* Equal to a C string; frees `s` (an owned result). */
static bool take_eq(uniffi_string s, const char *expected) {
    bool eq = s.len == strlen(expected) && memcmp(s.data, expected, s.len) == 0 && s.data[s.len] == '\0';
    if (!eq) {
        fprintf(stderr, "  got \"%.*s\", expected \"%s\"\n", (int)s.len, s.data ? s.data : "", expected);
    }
    uniffi_string_free(&s);
    return eq;
}

static bool str_eq(uniffi_string s, const char *expected) {
    return s.len == strlen(expected) && memcmp(s.data, expected, s.len) == 0;
}

static void test_primitives(void) {
    CHECK(tour_add_u8(200, 100, NULL) == 44);
    CHECK(tour_add_i32(-5, 3, NULL) == -2);
    CHECK(tour_add_u64(1ULL << 40, 5, NULL) == (1ULL << 40) + 5);
    CHECK(tour_negate_i64(9, NULL) == -9);
    CHECK(tour_half_f32(3.0f, NULL) == 1.5f);
    CHECK(tour_half_f64(5.0, NULL) == 2.5);
    CHECK(tour_invert(true, NULL) == false);
    CHECK(tour_invert(false, NULL) == true);
    CHECK(take_eq(tour_shout(uniffi_str("h\xc3\xa9llo"), NULL), "H\xc3\x89LLO"));
    CHECK(take_eq(tour_shout(uniffi_str(""), NULL), ""));
    uint8_t in[] = {1, 2, 3};
    uniffi_bytes reversed = tour_reverse_bytes(uniffi_bytes_view(in, 3), NULL);
    CHECK(reversed.len == 3 && reversed.data[0] == 3 && reversed.data[2] == 1);
    uniffi_bytes_free(&reversed);
    tour_nothing(NULL);
    CHECK(take_eq(tour_greet(uniffi_str("Ada"), uniffi_str("Hello"), NULL), "Hello, Ada!"));
    CHECK(take_eq(tour_greet(uniffi_str("Ada"), uniffi_str("Hi"), NULL), "Hi, Ada!"));
}

static void test_containers(void) {
    tour_OptionI32 four = {true, 4};
    tour_OptionI32 eight = tour_maybe_double(four, NULL);
    CHECK(eight.has_value && eight.value == 8);
    tour_OptionI32 none = {0};
    CHECK(!tour_maybe_double(none, NULL).has_value);

    int64_t values[] = {1, 2, 3, 4};
    tour_SequenceI64 seq = {values, 4};
    CHECK(tour_sum(seq, NULL) == 10);
    tour_SequenceI64 empty = {NULL, 0};
    CHECK(tour_sum(empty, NULL) == 0);

    uniffi_string words[] = {uniffi_str("a"), uniffi_str("bcd"), uniffi_str("ef")};
    tour_SequenceString word_seq = {words, 3};
    tour_MapStringU32 lengths = tour_word_lengths(word_seq, NULL);
    CHECK(lengths.len == 3);
    bool found = false;
    for (size_t i = 0; i < lengths.len; ++i) {
        if (str_eq(lengths.keys[i], "bcd")) {
            found = lengths.values[i] == 3;
        }
    }
    CHECK(found);
    tour_MapStringU32_free(&lengths);
    CHECK(lengths.len == 0 && lengths.keys == NULL);

    uniffi_string xy[] = {uniffi_str("x"), uniffi_str("y")};
    tour_OptionSequenceString items[3] = {{false, {NULL, 0}}, {true, {xy, 2}}, {true, {NULL, 0}}};
    tour_SequenceOptionSequenceString n = {items, 3};
    tour_SequenceOptionSequenceString back = tour_nested(n, NULL);
    CHECK(back.len == 3 && !back.items[0].has_value);
    CHECK(back.items[1].has_value && back.items[1].value.len == 2 && str_eq(back.items[1].value.items[1], "y"));
    CHECK(back.items[2].has_value && back.items[2].value.len == 0);
    tour_SequenceOptionSequenceString_free(&back);

    uint64_t a[] = {1, 2};
    uniffi_string keys[] = {uniffi_str("a"), uniffi_str("b")};
    tour_SequenceU64 vals[] = {{a, 2}, {NULL, 0}};
    tour_MapStringSequenceU64 m = {keys, vals, 2};
    tour_MapStringSequenceU64 echoed = tour_echo_u64s(m, NULL);
    CHECK(echoed.len == 2);
    for (size_t i = 0; i < echoed.len; ++i) {
        if (str_eq(echoed.keys[i], "a")) {
            CHECK(echoed.values[i].len == 2 && echoed.values[i].items[1] == 2);
        } else {
            CHECK(str_eq(echoed.keys[i], "b") && echoed.values[i].len == 0);
        }
    }
    tour_MapStringSequenceU64_free(&echoed);

    uniffi_timestamp t = tour_timestamp_plus((uniffi_timestamp){100, 0}, (uniffi_duration){2, 500000000}, NULL);
    CHECK(t.secs == 102 && t.nanos == 500000000);
    uniffi_timestamp before = tour_timestamp_plus((uniffi_timestamp){-10, 0}, (uniffi_duration){4, 0}, NULL);
    CHECK(before.secs == -6 && before.nanos == 0);
    uniffi_timestamp half = tour_timestamp_plus((uniffi_timestamp){-10, 0}, (uniffi_duration){4, 250000000}, NULL);
    CHECK(half.secs == -6 && half.nanos == 250000000);
    uniffi_duration d = tour_double_duration((uniffi_duration){1, 500000000}, NULL);
    CHECK(d.secs == 3 && d.nanos == 0);
}

static void test_records_and_enums(void) {
    CHECK(tour_mix(tour_Color_Red, NULL) == tour_Color_Green);
    CHECK(tour_mix(tour_Color_Blue, NULL) == tour_Color_Red);

    tour_Shape circle = tour_make_shape(0, NULL);
    CHECK(circle.tag == tour_Shape_Circle && circle.u.Circle.radius == 2.0 && circle.u.Circle.center.y == 2.0);
    tour_Shape rect = tour_make_shape(1, NULL);
    CHECK(rect.tag == tour_Shape_Rect && rect.u.Rect.v1.x == 2.0);
    CHECK(tour_make_shape(2, NULL).tag == tour_Shape_Empty);
    CHECK(tour_area(circle, NULL) == 12.0);
    CHECK(tour_area(rect, NULL) == 6.0);
    tour_Shape empty = {tour_Shape_Empty, {{{0, 0}, 0}}};
    CHECK(tour_area(empty, NULL) == 0.0);
    tour_Shape unit = {tour_Shape_Circle, {{{0, 0}, 1.0}}};
    CHECK(tour_area(unit, NULL) == 3.0);

    /* Defaults through the designated initializer. */
    tour_Settings s = tour_Settings_DEFAULTS;
    s.name = uniffi_str("n");
    s.origin = (tour_Point){7.0, 8.0};
    CHECK(take_eq(tour_describe(s, NULL), "n:8:None::7"));
    uniffi_string tags[] = {uniffi_str("a"), uniffi_str("b")};
    s.retries = 2;
    s.label = (tour_OptionString){true, uniffi_str("L")};
    s.tags = (tour_SequenceString){tags, 2};
    CHECK(take_eq(tour_describe(s, NULL), "n:2:Some(\"L\"):a+b:7"));
}

static void test_errors(void) {
    uniffi_status st = {0};
    tour_DataError err;
    memset(&err, 0, sizeof err);

    tour_fail_data(0, &err, &st);
    CHECK(st.code == UNIFFI_OK);
    tour_fail_data(1, &err, &st);
    CHECK(st.code == UNIFFI_ERROR && err.tag == tour_DataError_BadCode && err.u.BadCode.code == 42);
    CHECK(str_eq(st.message, "BadCode"));
    tour_DataError_free(&err);
    tour_fail_data(2, &err, &st);
    CHECK(st.code == UNIFFI_ERROR && err.tag == tour_DataError_TooLong && err.u.TooLong.v0 == 9);
    tour_fail_data(3, &err, &st);
    CHECK(st.code == UNIFFI_ERROR && err.tag == tour_DataError_Plain);
    /* No err out-parameter: the error is still reported, and freed. */
    tour_fail_data(3, NULL, &st);
    CHECK(st.code == UNIFFI_ERROR);

    tour_FlatError ferr;
    memset(&ferr, 0, sizeof ferr);
    CHECK(tour_fail_flat(0, &ferr, &st) == 7 && st.code == UNIFFI_OK);
    tour_fail_flat(1, &ferr, &st);
    CHECK(st.code == UNIFFI_ERROR && ferr.tag == tour_FlatError_NotFound);
    tour_fail_flat(2, &ferr, &st);
    CHECK(st.code == UNIFFI_ERROR && ferr.tag == tour_FlatError_Denied && str_eq(ferr.u.Denied.v0, "nope"));
    tour_FlatError_free(&ferr);

    tour_MessageError merr;
    memset(&merr, 0, sizeof merr);
    tour_fail_message(1, &merr, &st);
    CHECK(st.code == UNIFFI_ERROR && merr.tag == tour_MessageError_Io && str_eq(merr.message, "io failure: disk"));
    CHECK(str_eq(st.message, "Io: io failure: disk"));
    tour_MessageError_free(&merr);
    tour_fail_message(2, &merr, &st);
    CHECK(st.code == UNIFFI_ERROR && merr.tag == tour_MessageError_Closed);
    tour_MessageError_free(&merr);

    tour_panics(&st);
    CHECK(st.code == UNIFFI_PANIC && strstr(st.message.data, "boom") != NULL);
    /* A later success clears the status. */
    tour_nothing(&st);
    CHECK(st.code == UNIFFI_OK && st.message.data == NULL);
    uniffi_status_free(&st);
}

static void test_objects(void) {
    uniffi_status st = {0};
    tour_Counter counter = tour_Counter_new(10, NULL);
    CHECK(tour_Counter_get(counter, NULL) == 10);
    CHECK(tour_Counter_add(counter, 5, NULL) == 15);
    tour_Counter zero = tour_Counter_zero(NULL);
    CHECK(tour_Counter_get(zero, NULL) == 0);
    tour_Counter_free(zero);
    tour_FlatError ferr;
    memset(&ferr, 0, sizeof ferr);
    tour_Counter bad = tour_Counter_checked(-1, &ferr, &st);
    CHECK(st.code == UNIFFI_ERROR && ferr.tag == tour_FlatError_Denied && bad.handle == 0);
    tour_FlatError_free(&ferr);
    tour_Counter three = tour_Counter_checked(3, &ferr, &st);
    CHECK(st.code == UNIFFI_OK && tour_Counter_get(three, NULL) == 3);
    tour_Counter_free(three);

    tour_Counter other = tour_Counter_new(100, NULL);
    tour_Counter_merge(counter, other, NULL);
    CHECK(tour_Counter_get(counter, NULL) == 115);
    CHECK(tour_Counter_get(other, NULL) == 100); /* still usable after being passed to Rust */
    tour_Counter_free(other);

    tour_Counter twin = tour_Counter_twin(counter, NULL);
    CHECK(tour_Counter_get(twin, NULL) == 115 && twin.handle != counter.handle);
    CHECK(tour_Counter_eq(twin, counter, NULL));
    CHECK(!tour_Counter_ne(twin, counter, NULL));
    tour_OptionCounter maybe = tour_Counter_maybe_twin(counter, true, NULL);
    CHECK(maybe.has_value);
    tour_OptionCounter_free(&maybe);
    CHECK(!tour_Counter_maybe_twin(counter, false, NULL).has_value);
    CHECK(take_eq(tour_Counter_to_string(counter, NULL), "Counter(115)"));
    CHECK(tour_Counter_hash(counter, NULL) == tour_Counter_hash(twin, NULL));
    tour_Counter_free(twin);

    tour_DataError derr;
    memset(&derr, 0, sizeof derr);
    CHECK(tour_Counter_try_sub(counter, 15, &derr, &st) == 100 && st.code == UNIFFI_OK);
    tour_Counter_try_sub(counter, 1000, &derr, &st);
    CHECK(st.code == UNIFFI_ERROR && derr.tag == tour_DataError_BadCode);

    tour_Counter copy = tour_Counter_clone(counter);
    tour_Counter_free(counter);
    CHECK(tour_Counter_get(copy, NULL) == 100); /* a clone outlives the original handle */
    tour_Counter_free(copy);

    tour_Holder holder = tour_make_holder(1, NULL);
    CHECK(tour_Counter_get(holder.counter, NULL) == 1);
    CHECK(holder.others.len == 2 && tour_Counter_get(holder.others.items[1], NULL) == 3);
    tour_Holder_free(&holder);
    uniffi_status_free(&st);
}

typedef struct Recording {
    int names;
    char last[32];
    bool done;
} Recording;

static int32_t on_event(void *ctx, uniffi_string name, int32_t value, uniffi_status *status) {
    (void)status;
    Recording *rec = (Recording *)ctx;
    rec->names++;
    snprintf(rec->last, sizeof rec->last, "%.*s", (int)name.len, name.data);
    return value * 10;
}

static void on_done(void *ctx, uniffi_status *status) {
    (void)status;
    ((Recording *)ctx)->done = true;
}

static uniffi_string fallible(void *ctx, int32_t n, tour_FlatError *err, uniffi_status *status) {
    (void)ctx;
    uniffi_string empty = {NULL, 0};
    if (n < 0) {
        err->tag = tour_FlatError_Denied;
        err->u.Denied.v0 = uniffi_string_copy("negative", 8);
        status->code = UNIFFI_ERROR;
        return empty;
    }
    if (n == 0) {
        uniffi_fail_str(status, UNIFFI_PANIC, "zero");
        return empty;
    }
    char buf[16];
    int len = snprintf(buf, sizeof buf, "ok%d", (int)n);
    return uniffi_string_copy(buf, (size_t)len);
}

static const tour_Listener_vtable listener_vtable = {on_event, on_done, fallible};

static void test_callbacks(void) {
    Recording rec = {0, "", false};
    tour_Listener listener = {&listener_vtable, &rec};
    CHECK(tour_notify(listener, 4, NULL) == 60);
    CHECK(rec.done);
    CHECK(rec.names == 4 && strcmp(rec.last, "event-3") == 0);

    uniffi_status st = {0};
    tour_FlatError err;
    memset(&err, 0, sizeof err);
    CHECK(take_eq(tour_try_listener(listener, 5, &err, &st), "ok5"));
    tour_try_listener(listener, -1, &err, &st);
    CHECK(st.code == UNIFFI_ERROR && err.tag == tour_FlatError_Denied && str_eq(err.u.Denied.v0, "negative"));
    tour_FlatError_free(&err);
    tour_try_listener(listener, 0, &err, &st);
    CHECK(st.code == UNIFFI_PANIC && strstr(st.message.data, "zero") != NULL);
    uniffi_status_free(&st);
}

static int released = 0;

static uniffi_string c_greet(void *ctx, uniffi_string name, uniffi_status *status) {
    (void)status;
    const char *who = (const char *)ctx;
    char buf[64];
    int len = snprintf(buf, sizeof buf, "%s says hi to %.*s", who, (int)name.len, name.data);
    return uniffi_string_copy(buf, (size_t)len);
}

static void c_release(void *ctx) {
    (void)ctx;
    released++;
}

static const tour_Greeter_vtable greeter_vtable = {c_greet, c_release};

static void test_trait_objects(void) {
    tour_Greeter rust = tour_rust_greeter(NULL);
    CHECK(take_eq(tour_Greeter_greet(rust, uniffi_str("x"), NULL), "rust says hi to x"));
    CHECK(take_eq(tour_use_greeter(rust, uniffi_str("y"), NULL), "rust says hi to y"));
    tour_Greeter_free(rust);

    tour_Greeter c = tour_Greeter_from_impl(&greeter_vtable, (void *)"c");
    CHECK(take_eq(tour_Greeter_greet(c, uniffi_str("w"), NULL), "c says hi to w"));
    CHECK(take_eq(tour_use_greeter(c, uniffi_str("z"), NULL), "c says hi to z"));
    tour_Greeter c2 = tour_Greeter_clone(c);
    tour_Greeter_free(c);
    CHECK(released == 0); /* c2 still holds it */
    tour_Greeter_free(c2);
    CHECK(released == 1);
}

static uniffi_string c_fetch(void *ctx, uniffi_string key, uniffi_status *status) {
    (void)ctx;
    (void)status;
    char buf[64];
    int len = snprintf(buf, sizeof buf, "fetched %.*s", (int)key.len, key.data);
    return uniffi_string_copy(buf, (size_t)len);
}

static const tour_AsyncSource_vtable source_vtable = {c_fetch};

typedef struct Pending {
    uniffi_mutex mutex;
    uniffi_cond cond;
    int remaining;
    int ok;
} Pending;

static void add_done(void *ctx, int32_t result, uniffi_status *status) {
    Pending *p = (Pending *)ctx;
    uniffi_mutex_lock(&p->mutex);
    if (status->code == UNIFFI_OK && result % 2 == 0) {
        p->ok++;
    }
    p->remaining--;
    uniffi_cond_signal(&p->cond);
    uniffi_mutex_unlock(&p->mutex);
}

static void test_async(void) {
    CHECK(tour_slow_add(2, 3, NULL) == 5);
    uniffi_status st = {0};
    tour_DataError err;
    memset(&err, 0, sizeof err);
    CHECK(take_eq(tour_slow_fail(false, &err, &st), "fine"));
    tour_slow_fail(true, &err, &st);
    CHECK(st.code == UNIFFI_ERROR && err.tag == tour_DataError_Plain);
    tour_slow_unit(NULL);

    tour_Worker worker = tour_Worker_new(uniffi_str("w"), NULL);
    CHECK(take_eq(tour_Worker_work(worker, 3, NULL), "w did 3"));
    tour_Worker_free(worker);

    /* Many at once, with completion callbacks. */
    Pending p;
    uniffi_mutex_init(&p.mutex);
    uniffi_cond_init(&p.cond);
    p.remaining = 50;
    p.ok = 0;
    for (int i = 0; i < 50; ++i) {
        tour_slow_add_async(i, i, add_done, &p);
    }
    uniffi_mutex_lock(&p.mutex);
    while (p.remaining > 0) {
        uniffi_cond_wait(&p.cond, &p.mutex);
    }
    uniffi_mutex_unlock(&p.mutex);
    CHECK(p.ok == 50);

    tour_AsyncSource source = {&source_vtable, NULL};
    CHECK(take_eq(tour_fetch_via(source, uniffi_str("k"), NULL), "fetched k"));
    uniffi_status_free(&st);
}

static void test_custom_types(void) {
    CHECK(tour_next_handle(41, NULL) == 42);
    CHECK(take_eq(tour_host_of(uniffi_str("https://example.com/a/b"), NULL), "example.com"));
}

static void test_uniffi_032(void) {
    /* Recursive enum through Box: Add(Neg(Num 2), Num 5). */
    tour_Expr two = {tour_Expr_Num, {.Num = {2}}};
    tour_Expr neg = {tour_Expr_Neg, {.Neg = {&two}}};
    tour_Expr five = {tour_Expr_Num, {.Num = {5}}};
    tour_Expr add;
    add.tag = tour_Expr_Add;
    add.u.Add.left = &neg;
    add.u.Add.right = &five;
    CHECK(tour_eval(add, NULL) == 3);

    int64_t nums[] = {1, 2, 3, 4};
    tour_Expr sum = tour_sum_expr((tour_SequenceI64){nums, 4}, NULL);
    CHECK(tour_eval(sum, NULL) == 10);
    CHECK(sum.tag == tour_Expr_Add && sum.u.Add.right->tag == tour_Expr_Num && sum.u.Add.right->u.Num.value == 4);
    tour_Expr_free(&sum);

    tour_TreeNode leaf = {uniffi_str("leaf"), {NULL, 0}, {false, NULL}};
    tour_TreeNode kids[] = {leaf, leaf};
    tour_TreeNode sibling = leaf;
    tour_TreeNode tree = {uniffi_str("root"), {kids, 2}, {true, &sibling}};
    CHECK(tour_count_nodes(tree, NULL) == 4);

    tour_SetString words = tour_unique_words(uniffi_str("a b a c b"), NULL);
    CHECK(words.len == 3);
    tour_SetString_free(&words);
    uint32_t set[] = {1, 2, 3};
    CHECK(tour_set_sum((tour_SetU32){set, 3}, NULL) == 6);
    CHECK(tour_set_sum((tour_SetU32){NULL, 0}, NULL) == 0);

    static uint8_t bytes[100000];
    memset(bytes, 1, sizeof bytes);
    CHECK(tour_checksum(uniffi_bytes_view(bytes, sizeof bytes), NULL) == 100000);
    CHECK(tour_checksum(uniffi_bytes_view(NULL, 0), NULL) == 0);
    tour_Adler adler = tour_Adler_new(NULL);
    uint8_t one_two[] = {1, 2};
    uint8_t three[] = {3};
    tour_Adler_update(adler, uniffi_bytes_view(one_two, 2), NULL);
    tour_Adler_update(adler, uniffi_bytes_view(three, 1), NULL);
    CHECK(tour_Adler_value(adler, NULL) == (1u * 31 + 2) * 31 + 3);
    tour_Adler_free(adler);
}

int main(void) {
    test_primitives();
    test_containers();
    test_records_and_enums();
    test_errors();
    test_objects();
    test_callbacks();
    test_trait_objects();
    test_async();
    test_custom_types();
    test_uniffi_032();
    printf("%d checks, %d failures\n", checks, failures);
    return failures == 0 ? 0 : 1;
}
