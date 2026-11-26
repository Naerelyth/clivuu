/* Test-only ABI observation shim. No proprietary resources or code included. */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static const char *class_name(void *object) {
    typedef const void *(*Meta)(const void *);
    typedef const char *(*Name)(const void *);
    Name name = (Name)dlsym(RTLD_NEXT, "_ZNK11QMetaObject9classNameEv");
    Meta meta = (Meta)(*(void ***)object)[0];
    return name ? name(meta(object)) : NULL;
}

static void geometry_note(void *object, const char *operation, const int *values,
                          int count, void *caller) {
    const char *name = class_name(object), *path = getenv("CLIVUU_REFERENCE_LOG");
    static unsigned observations;
    if (!path || !name || observations++ >= 2000) return;
    Dl_info info = {0};
    dladdr(caller, &info);
    FILE *file = fopen(path, "a");
    if (!file) return;
    if (info.dli_fname && strstr(info.dli_fname, "/Snipaste") &&
        (unsigned long)info.dli_fbase == 0x400000)
        fprintf(file, "reference-scale %.6f %.6f\n",
                *(const double *)0xd7f2c0, *(const double *)0xd7f2c8);
    fprintf(file, "%s %s", operation, name);
    for (int i = 0; i < count; i++) fprintf(file, " %d", values[i]);
    fprintf(file, " caller=%s+0x%lx\n", info.dli_fname ? info.dli_fname : "?",
            (unsigned long)((char *)caller - (char *)info.dli_fbase));
    fclose(file);
}

/* Qt 6 x86_64 ABI: QPoint is two ints; QRect is inclusive left/top/right/bottom. */
void widget_move(void *object, const int *point) __asm__("_ZN7QWidget4moveERK6QPoint");
void widget_move(void *object, const int *point) {
    typedef void (*Real)(void *, const int *);
    geometry_note(object, "move", point, 2, __builtin_return_address(0));
    ((Real)dlsym(RTLD_NEXT, "_ZN7QWidget4moveERK6QPoint"))(object, point);
}

void widget_geometry(void *object, const int *rect) __asm__("_ZN7QWidget11setGeometryERK5QRect");
void widget_geometry(void *object, const int *rect) {
    typedef void (*Real)(void *, const int *);
    /* At this observed x86_64 callsite R15 retains the layout-state pointer.
       Only inspect it for the exact verified ET_EXEC callsite, never Qt callers. */
    void *layout_state;
    __asm__("mov %%r15, %0" : "=r"(layout_state));
    Dl_info info = {0};
    void *caller = __builtin_return_address(0);
    dladdr(caller, &info);
    const char *path = getenv("CLIVUU_REFERENCE_LOG");
    const char *module = info.dli_fname ? strrchr(info.dli_fname, '/') : NULL;
    if (path && module && !strcmp(module, "/Snipaste") &&
        (char *)caller - (char *)info.dli_fbase == 0x3c155a) {
        FILE *file = fopen(path, "a");
        if (file) {
            const int *selection = (const int *)((char *)layout_state + 0x1b0);
            const int *avoid = (const int *)((char *)layout_state + 0x1c0);
            const int *viewport = (const int *)((char *)layout_state + 0x1d4);
            const int *spacing = (const int *)((char *)layout_state + 0x1e4);
            fprintf(file, "layout-state selection=%d,%d,%d,%d avoid=%d,%d,%d,%d viewport=%d,%d,%d,%d extra=%d margin=%d state=%d\n",
                selection[0], selection[1], selection[2], selection[3], avoid[0], avoid[1], avoid[2], avoid[3],
                viewport[0], viewport[1], viewport[2], viewport[3], spacing[0], spacing[2],
                *(int *)((char *)layout_state + 0x1d0));
            fclose(file);
        }
    }
    geometry_note(object, "geometry-ltrb", rect, 4, __builtin_return_address(0));
    ((Real)dlsym(RTLD_NEXT, "_ZN7QWidget11setGeometryERK5QRect"))(object, rect);
}

void graphics_position(void *object, const double *point) __asm__("_ZN13QGraphicsItem6setPosERK7QPointF");
void graphics_position(void *object, const double *point) {
    typedef void (*Real)(void *, const double *);
    static unsigned count;
    const char *path = getenv("CLIVUU_REFERENCE_LOG");
    void *caller = __builtin_return_address(0);
    Dl_info info = {0}; dladdr(caller, &info);
    if (path && count++ < 2000) {
        FILE *file = fopen(path, "a");
        if (file) {
            fprintf(file, "graphics-position %p %.2f %.2f caller=%s+0x%lx\n", object,
                    point[0], point[1], info.dli_fname ? info.dli_fname : "?",
                    (unsigned long)((char *)caller - (char *)info.dli_fbase));
            fclose(file);
        }
    }
    ((Real)dlsym(RTLD_NEXT, "_ZN13QGraphicsItem6setPosERK7QPointF"))(object, point);
}

static void note(const char *message, const char *name, int value) {
    const char *path = getenv("CLIVUU_REFERENCE_LOG");
    if (!path) return;
    FILE *file = fopen(path, "a");
    if (file) { fprintf(file, "%s %s %d\n", message, name ? name : "?", value); fclose(file); }
}

void visible(void *object, bool value) __asm__("_ZN7QWidget10setVisibleEb");
void visible(void *object, bool value) {
    typedef void (*Real)(void *, bool);
    typedef const void *(*Meta)(const void *);
    typedef const char *(*Name)(const void *);
    Real real = (Real)dlsym(RTLD_NEXT, "_ZN7QWidget10setVisibleEb");
    Name name = (Name)dlsym(RTLD_NEXT, "_ZNK11QMetaObject9classNameEv");
    Meta meta = (Meta)(*(void ***)object)[0];
    note("visible", name ? name(meta(object)) : NULL, value);
    real(object, value);
}

bool read_image(void *reader, void *image) __asm__("_ZN12QImageReader4readEP6QImage");
bool read_image(void *reader, void *image) {
    typedef bool (*Real)(void *, void *);
    Real real = (Real)dlsym(RTLD_NEXT, "_ZN12QImageReader4readEP6QImage");
    bool result = real(reader, image);
    note("image-read", "QImageReader", result);
    return result;
}

void graphics_scale(void *object, double value) __asm__("_ZN13QGraphicsItem8setScaleEd");
void graphics_scale(void *object, double value) {
    typedef void (*Real)(void *, double);
    ((Real)dlsym(RTLD_NEXT,"_ZN13QGraphicsItem8setScaleEd"))(object,value);
    const char *path=getenv("CLIVUU_REFERENCE_LOG");static unsigned count;
    if(path&&count++<600){
        double rect[4]={0};typedef void (*Rect)(double *,const void *);
        ((Rect)dlsym(RTLD_NEXT,"_ZNK13QGraphicsItem17sceneBoundingRectEv"))(rect,object);
        FILE *file=fopen(path,"a");if(file){fprintf(file,"graphics-scale %p %.8f rect=%.3f,%.3f,%.3f,%.3f\n",object,value,rect[0],rect[1],rect[2],rect[3]);fclose(file);}
    }
}
