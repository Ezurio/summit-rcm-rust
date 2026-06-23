#define _DEFAULT_SOURCE

#include <errno.h>
#include <fcntl.h>
#include <linux/uhid.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
#include <unistd.h>

/*
 * Expose a hidraw-capable vendor-defined device rather than a real keyboard.
 * The test service reads the raw 8-byte reports directly from hidraw and does
 * its own HID keycode decoding, so the descriptor does not need to advertise a
 * desktop keyboard to the host input stack.
 */
static const uint8_t BARCODE_REPORT_DESCRIPTOR[] = {
    0x06, 0x00, 0xFF,
    0x09, 0x01,
    0xA1, 0x01,
    0x15, 0x00,
    0x26, 0xFF, 0x00,
    0x75, 0x08,
    0x95, 0x08,
    0x09, 0x01,
    0x81, 0x02,
    0xC0,
};

enum hid_usage_key {
    HID_USAGE_KEY_A = 4,
    HID_USAGE_KEY_1 = 30,
    HID_USAGE_KEY_0 = 39,
    HID_USAGE_KEY_ENTER = 40,
    HID_USAGE_KEY_SPACE = 44,
    HID_USAGE_KEY_MINUS = 45,
};

struct hid_key_event {
    uint8_t modifier;
    uint8_t keycode;
};

static void usage(void) {
    fprintf(stderr,
            "Usage: uhid_barcode_scanner_sim [--uniq AA:BB:CC:DD:EE:FF] [--barcode ABC123] "
            "[--send-delay-ms 3000] [--hold-ms 3000]\n");
}

static int write_event(int fd, const struct uhid_event *event) {
    ssize_t written = write(fd, event, sizeof(*event));
    if (written < 0) {
        return -errno;
    }
    if ((size_t)written != sizeof(*event)) {
        return -EIO;
    }
    return 0;
}

static int create_device(int fd, const char *uniq) {
    struct uhid_event event;

    memset(&event, 0, sizeof(event));
    event.type = UHID_CREATE2;
    snprintf((char *)event.u.create2.name, sizeof(event.u.create2.name), "%s", "Summit UHID Scanner");
    snprintf((char *)event.u.create2.phys, sizeof(event.u.create2.phys), "%s", "uhid/summit_scanner");
    snprintf((char *)event.u.create2.uniq, sizeof(event.u.create2.uniq), "%s", uniq);
    event.u.create2.rd_size = sizeof(BARCODE_REPORT_DESCRIPTOR);
    memcpy(event.u.create2.rd_data, BARCODE_REPORT_DESCRIPTOR, sizeof(BARCODE_REPORT_DESCRIPTOR));
    event.u.create2.bus = BUS_BLUETOOTH;
    event.u.create2.vendor = 0x0001;
    event.u.create2.product = 0x0001;
    event.u.create2.version = 0x0001;
    event.u.create2.country = 0;

    return write_event(fd, &event);
}

static int destroy_device(int fd) {
    struct uhid_event event;

    memset(&event, 0, sizeof(event));
    event.type = UHID_DESTROY;
    return write_event(fd, &event);
}

static int send_input_report(int fd, uint8_t modifier, uint8_t keycode) {
    struct uhid_event event;

    memset(&event, 0, sizeof(event));
    event.type = UHID_INPUT2;
    event.u.input2.size = 8;
    event.u.input2.data[0] = modifier;
    event.u.input2.data[1] = 0;
    event.u.input2.data[2] = keycode;

    return write_event(fd, &event);
}

static bool ascii_to_hid(char ch, struct hid_key_event *event) {
    memset(event, 0, sizeof(*event));

    if (ch >= 'a' && ch <= 'z') {
        event->keycode = (uint8_t)(HID_USAGE_KEY_A + (ch - 'a'));
        return true;
    }
    if (ch >= 'A' && ch <= 'Z') {
        event->modifier = 0x02;
        event->keycode = (uint8_t)(HID_USAGE_KEY_A + (ch - 'A'));
        return true;
    }
    if (ch >= '1' && ch <= '9') {
        event->keycode = (uint8_t)(HID_USAGE_KEY_1 + (ch - '1'));
        return true;
    }
    if (ch == '0') {
        event->keycode = HID_USAGE_KEY_0;
        return true;
    }
    if (ch == ' ') {
        event->keycode = HID_USAGE_KEY_SPACE;
        return true;
    }
    if (ch == '-') {
        event->keycode = HID_USAGE_KEY_MINUS;
        return true;
    }
    if (ch == '_') {
        event->modifier = 0x02;
        event->keycode = HID_USAGE_KEY_MINUS;
        return true;
    }

    return false;
}

static int emit_barcode(int fd, const char *barcode) {
    const unsigned int key_delay_us = 20000;

    for (const char *cursor = barcode; *cursor != '\0'; ++cursor) {
        struct hid_key_event event;
        int ret;

        if (!ascii_to_hid(*cursor, &event)) {
            fprintf(stderr, "Unsupported barcode character: %c\n", *cursor);
            return -EINVAL;
        }

        ret = send_input_report(fd, event.modifier, event.keycode);
        if (ret < 0) {
            return ret;
        }
        usleep(key_delay_us);

        ret = send_input_report(fd, 0, 0);
        if (ret < 0) {
            return ret;
        }
        usleep(key_delay_us);
    }

    if (send_input_report(fd, 0, HID_USAGE_KEY_ENTER) < 0) {
        return -EIO;
    }
    usleep(key_delay_us);

    if (send_input_report(fd, 0, 0) < 0) {
        return -EIO;
    }

    return 0;
}

int main(int argc, char **argv) {
    const char *uniq = "AA:BB:CC:DD:EE:FF";
    const char *barcode = "ABC123";
    int send_delay_ms = 3000;
    int hold_ms = 3000;
    int fd;
    int ret;

    for (int index = 1; index < argc; ++index) {
        if (strcmp(argv[index], "--uniq") == 0 && index + 1 < argc) {
            uniq = argv[++index];
        } else if (strcmp(argv[index], "--barcode") == 0 && index + 1 < argc) {
            barcode = argv[++index];
        } else if (strcmp(argv[index], "--send-delay-ms") == 0 && index + 1 < argc) {
            send_delay_ms = atoi(argv[++index]);
        } else if (strcmp(argv[index], "--hold-ms") == 0 && index + 1 < argc) {
            hold_ms = atoi(argv[++index]);
        } else {
            usage();
            return 2;
        }
    }

    fd = open("/dev/uhid", O_RDWR | O_CLOEXEC);
    if (fd < 0) {
        perror("open /dev/uhid");
        return 1;
    }

    ret = create_device(fd, uniq);
    if (ret < 0) {
        fprintf(stderr, "Failed to create UHID device: %s\n", strerror(-ret));
        close(fd);
        return 1;
    }

    fprintf(stdout, "Created UHID scanner with HID_UNIQ=%s barcode=%s\n", uniq, barcode);
    fflush(stdout);

    usleep((unsigned int)send_delay_ms * 1000U);

    ret = emit_barcode(fd, barcode);
    if (ret < 0) {
        fprintf(stderr, "Failed to emit barcode: %s\n", strerror(-ret));
        (void)destroy_device(fd);
        close(fd);
        return 1;
    }

    fprintf(stdout, "Emitted barcode payload, holding device for %d ms\n", hold_ms);
    fflush(stdout);
    usleep((unsigned int)hold_ms * 1000U);

    ret = destroy_device(fd);
    if (ret < 0) {
        fprintf(stderr, "Failed to destroy UHID device: %s\n", strerror(-ret));
        close(fd);
        return 1;
    }

    close(fd);
    fprintf(stdout, "Destroyed UHID scanner\n");
    return 0;
}