#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <jni.h>
#include <linux/uinput.h>
#include <stdio.h>
#include <string.h>
#include <sys/ioctl.h>
#include <unistd.h>

static int pointer_fd = -1;

int nbpointer_move_rel(int dx, int dy);

static int write_event(int type, int code, int value) {
    struct input_event ev;
    memset(&ev, 0, sizeof(ev));
    ev.type = (unsigned short) type;
    ev.code = (unsigned short) code;
    ev.value = value;
    if (write(pointer_fd, &ev, sizeof(ev)) != (ssize_t) sizeof(ev)) {
        return -1;
    }
    return 0;
}

static int write_sync(void) {
    return write_event(EV_SYN, SYN_REPORT, 0);
}

static int await_registered(void) {
    int tries = 0;
    while (tries < 60) {
        int found = 0;
        DIR *dir = opendir("/sys/class/input");
        if (dir != 0) {
            struct dirent *entry;
            while ((entry = readdir(dir)) != 0) {
                if (strncmp(entry->d_name, "input", 5) != 0) {
                    continue;
                }
                char path[256];
                snprintf(path, sizeof(path), "/sys/class/input/%s/name", entry->d_name);
                int fd = open(path, O_RDONLY);
                if (fd < 0) {
                    continue;
                }
                char name[64];
                ssize_t n = read(fd, name, sizeof(name) - 1);
                close(fd);
                if (n > 0) {
                    name[n] = 0;
                    if (strstr(name, "Northbridge Pointer") != 0) {
                        found = 1;
                        break;
                    }
                }
            }
            closedir(dir);
        }
        if (found) {
            return 0;
        }
        usleep(50000);
        tries++;
    }
    return -1;
}

int nbpointer_create(void) {
    if (pointer_fd >= 0) {
        return 0;
    }
    int fd = open("/dev/uinput", O_WRONLY | O_NONBLOCK);
    if (fd < 0) {
        return -errno;
    }
    int ok = 0;
    if (ioctl(fd, UI_SET_EVBIT, EV_KEY) < 0) ok = -1;
    if (ok == 0 && ioctl(fd, UI_SET_KEYBIT, BTN_LEFT) < 0) ok = -1;
    if (ok == 0 && ioctl(fd, UI_SET_KEYBIT, BTN_RIGHT) < 0) ok = -1;
    if (ok == 0 && ioctl(fd, UI_SET_KEYBIT, BTN_MIDDLE) < 0) ok = -1;
    if (ok == 0 && ioctl(fd, UI_SET_EVBIT, EV_REL) < 0) ok = -1;
    if (ok == 0 && ioctl(fd, UI_SET_RELBIT, REL_X) < 0) ok = -1;
    if (ok == 0 && ioctl(fd, UI_SET_RELBIT, REL_Y) < 0) ok = -1;
    if (ok == 0 && ioctl(fd, UI_SET_RELBIT, REL_WHEEL) < 0) ok = -1;
    if (ok == 0) {
        struct uinput_setup setup;
        memset(&setup, 0, sizeof(setup));
        setup.id.bustype = BUS_USB;
        setup.id.vendor = 0x1234;
        setup.id.product = 0x9001;
        setup.id.version = 1;
        strncpy(setup.name, "Northbridge Pointer", UINPUT_MAX_NAME_SIZE - 1);
        if (ioctl(fd, UI_DEV_SETUP, &setup) < 0) ok = -1;
    }
    if (ok == 0 && ioctl(fd, UI_DEV_CREATE) < 0) ok = -1;
    if (ok != 0) {
        int saved = errno;
        close(fd);
        return -saved;
    }
    pointer_fd = fd;
    if (await_registered() == 0) {
        usleep(250000);
        nbpointer_move_rel(-8190, -8190);
        nbpointer_move_rel(-8190, -8190);
    }
    return 0;
}

int nbpointer_move_rel(int dx, int dy) {
    if (pointer_fd < 0) {
        return -ENODEV;
    }
    if (dx != 0 && write_event(EV_REL, REL_X, dx) < 0) {
        return -EIO;
    }
    if (dy != 0 && write_event(EV_REL, REL_Y, dy) < 0) {
        return -EIO;
    }
    if (write_sync() < 0) {
        return -EIO;
    }
    return 0;
}

int nbpointer_button(int code, int down) {
    if (pointer_fd < 0) {
        return -ENODEV;
    }
    if (code != BTN_LEFT && code != BTN_RIGHT && code != BTN_MIDDLE) {
        return -EINVAL;
    }
    if (write_event(EV_KEY, code, down ? 1 : 0) < 0) {
        return -EIO;
    }
    if (write_sync() < 0) {
        return -EIO;
    }
    return 0;
}

int nbpointer_wheel(int delta) {
    if (pointer_fd < 0) {
        return -ENODEV;
    }
    if (delta == 0) {
        return 0;
    }
    if (write_event(EV_REL, REL_WHEEL, delta) < 0) {
        return -EIO;
    }
    if (write_sync() < 0) {
        return -EIO;
    }
    return 0;
}

void nbpointer_destroy(void) {
    if (pointer_fd < 0) {
        return;
    }
    ioctl(pointer_fd, UI_DEV_DESTROY);
    close(pointer_fd);
    pointer_fd = -1;
}

jint Java_bit_kedybaykedy_northbridge_input_PointerJni_nbCreate(JNIEnv *env, jclass clazz) {
    (void) env;
    (void) clazz;
    return (jint) nbpointer_create();
}

jint Java_bit_kedybaykedy_northbridge_input_PointerJni_nbMoveRel(
    JNIEnv *env, jclass clazz, jint dx, jint dy) {
    (void) env;
    (void) clazz;
    return (jint) nbpointer_move_rel((int) dx, (int) dy);
}

jint Java_bit_kedybaykedy_northbridge_input_PointerJni_nbButton(
    JNIEnv *env, jclass clazz, jint code, jboolean down) {
    (void) env;
    (void) clazz;
    return (jint) nbpointer_button((int) code, down == JNI_TRUE ? 1 : 0);
}

jint Java_bit_kedybaykedy_northbridge_input_PointerJni_nbWheel(
    JNIEnv *env, jclass clazz, jint delta) {
    (void) env;
    (void) clazz;
    return (jint) nbpointer_wheel((int) delta);
}

void Java_bit_kedybaykedy_northbridge_input_PointerJni_nbDestroy(JNIEnv *env, jclass clazz) {
    (void) env;
    (void) clazz;
    nbpointer_destroy();
}
