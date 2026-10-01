/* Host-side shim for the actual mount implementation extracted from our patch.
 * do_mount deliberately performs Linux's page-end write under AddressSanitizer.
 */
#include <assert.h>
#include <errno.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define PAGE_SIZE 4096
#define GFP_KERNEL 0
#define MS_SILENT 32768

struct list_head {
    struct list_head *next, *prev;
};
#define LIST_HEAD(name) struct list_head name = { &name, &name }
#define DEFINE_MUTEX(name) int name
#define container_of(ptr, type, member) \
    ((type *)((char *)(ptr) - offsetof(type, member)))
#define list_for_each_entry(pos, head, member) \
    for (struct list_head *node = (head)->next; \
         node != (head) && ((pos) = container_of(node, __typeof__(*(pos)), member), 1); \
         node = node->next)

static void list_add_tail(struct list_head *entry, struct list_head *head) {
    entry->prev = head->prev;
    entry->next = head;
    head->prev->next = entry;
    head->prev = entry;
}
static void mutex_lock(int *mutex) { assert(*mutex == 0); *mutex = 1; }
static void mutex_unlock(int *mutex) { assert(*mutex == 1); *mutex = 0; }

static int allocation_attempts, fail_allocation, live_allocations;
static void *test_allocate(size_t size) {
    allocation_attempts++;
    if (allocation_attempts == fail_allocation) return NULL;
    void *result = calloc(1, size);
    assert(result != NULL);
    live_allocations++;
    return result;
}
static void *kzalloc(size_t size, int flags) {
    (void)flags;
    return test_allocate(size);
}
static char *kstrdup(const char *value, int flags) {
    (void)flags;
    size_t size = strlen(value) + 1;
    char *result = test_allocate(size);
    if (result) memcpy(result, value, size);
    return result;
}
static void kfree(void *value) {
    if (!value) return;
    live_allocations--;
    assert(live_allocations >= 0);
    free(value);
}

static int mount_calls, mount_result;
static long do_mount(const char *directory, const char *mount_point,
                     const char *type, unsigned long flags, void *data) {
    mount_calls++;
    assert(mount_point[0] == '/');
    assert(strcmp(type, "hostfs") == 0);
    assert(flags == MS_SILENT);
    assert(data != (const void *)directory);
    assert(strcmp(data, directory) == 0);
    /* Mirror fs/namespace.c:path_mount(), the instruction from the crash. */
    ((char *)data)[PAGE_SIZE - 1] = 0;
    assert(strcmp(data, directory) == 0);
    return mount_result;
}

/* OPERIT_MOUNT_IMPLEMENTATION */

static void reset_state(void) {
    while (operit_runtime_mounts.next != &operit_runtime_mounts) {
        struct list_head *entry = operit_runtime_mounts.next;
        entry->next->prev = &operit_runtime_mounts;
        operit_runtime_mounts.next = entry->next;
        struct operit_runtime_mount *mount =
            container_of(entry, struct operit_runtime_mount, entry);
        kfree(mount->host_directory);
        kfree(mount->mount_point);
        kfree(mount);
    }
    assert(live_allocations == 0);
    assert(operit_runtime_mounts_lock == 0);
    allocation_attempts = fail_allocation = mount_calls = mount_result = 0;
}

int main(int argc, char **argv) {
    assert(argc == 2);
    const char *scenario = argv[1];
    const char *directory = "/private/var/mobile/Containers/Data/Application/test/Documents";
    if (strcmp(scenario, "success") == 0) {
        /* This is a read-only literal, as well as being much shorter than a page. */
        assert(linux_mount_app_directory(directory, directory) == 0);
        assert(mount_calls == 1);
        assert(live_allocations == 3); /* Mount record and its two strings only. */
        int attempts = allocation_attempts;
        assert(linux_mount_app_directory(directory, directory) == 0);
        assert(mount_calls == 1 && allocation_attempts == attempts);
        assert(linux_mount_app_directory("/other", directory) == -EBUSY);
        assert(mount_calls == 1 && live_allocations == 3);
    } else if (strcmp(scenario, "invalid") == 0) {
        assert(linux_mount_app_directory(NULL, directory) == -EINVAL);
        assert(linux_mount_app_directory(directory, NULL) == -EINVAL);
        assert(linux_mount_app_directory("", directory) == -EINVAL);
        assert(linux_mount_app_directory("relative", directory) == -EINVAL);
        assert(linux_mount_app_directory(directory, "relative") == -EINVAL);
        assert(mount_calls == 0 && allocation_attempts == 0);
    } else if (strcmp(scenario, "boundary") == 0) {
        char directory_page[PAGE_SIZE + 1];
        memset(directory_page, 'x', sizeof(directory_page));
        directory_page[0] = '/';
        directory_page[PAGE_SIZE - 1] = '\0';
        assert(linux_mount_app_directory(directory_page, "/boundary") == 0);
        assert(mount_calls == 1);
        reset_state();
        directory_page[PAGE_SIZE - 1] = 'x';
        directory_page[PAGE_SIZE] = '\0';
        assert(linux_mount_app_directory(directory_page, "/boundary") == -ENAMETOOLONG);
        assert(mount_calls == 0 && allocation_attempts == 0);
        /* Even without a NUL in the first page, the bounded scan must stop. */
        char unterminated[PAGE_SIZE];
        memset(unterminated, 'x', sizeof(unterminated));
        unterminated[0] = '/';
        assert(linux_mount_app_directory(unterminated, "/boundary") == -ENAMETOOLONG);
    } else if (strcmp(scenario, "allocation-failure") == 0) {
        /* Record, host string, mount string, and data page can each fail. */
        for (int fail = 1; fail <= 4; fail++) {
            fail_allocation = fail;
            assert(linux_mount_app_directory(directory, directory) == -ENOMEM);
            assert(mount_calls == 0 && live_allocations == 0);
            assert(operit_runtime_mounts_lock == 0);
            fail_allocation = 0;
            assert(linux_mount_app_directory(directory, directory) == 0);
            assert(mount_calls == 1 && live_allocations == 3);
            reset_state();
        }
    } else if (strcmp(scenario, "mount-failure") == 0) {
        mount_result = -EIO;
        assert(linux_mount_app_directory(directory, directory) == -EIO);
        assert(mount_calls == 1 && live_allocations == 0);
        assert(operit_runtime_mounts_lock == 0);
        mount_result = 0;
        assert(linux_mount_app_directory(directory, directory) == 0);
        assert(mount_calls == 2 && live_allocations == 3);
    } else {
        assert(!"unknown scenario");
    }
    reset_state();
    puts("mount regression passed");
    return 0;
}
