"""Give the private monitor fixture a deliberately distinct root cursor.

GTK controls must retain their themed cursor during implicit grabs instead of
inheriting this 96px cursor. Never run against the user's real display.
"""
import ctypes as c
import os
from pathlib import Path


class Image(c.Structure):
    _fields_ = [(name, c.c_uint) for name in
                ('version', 'size', 'width', 'height', 'xhot', 'yhot', 'delay')]
    _fields_ += [('pixels', c.POINTER(c.c_uint32))]


directory = Path(os.environ['CLIVUU_TEST_DIR'])
assert directory.name.startswith('clivuu-gnome-')
assert Path(os.environ['XDG_RUNTIME_DIR']) == directory / 'runtime'
xlib = c.CDLL('libX11.so.6')
cursor = c.CDLL('libXcursor.so.1')
xlib.XOpenDisplay.argtypes = [c.c_char_p]
xlib.XOpenDisplay.restype = c.c_void_p
xlib.XDefaultRootWindow.argtypes = [c.c_void_p]
xlib.XDefaultRootWindow.restype = c.c_ulong
xlib.XDefineCursor.argtypes = [c.c_void_p, c.c_ulong, c.c_ulong]
xlib.XFreeCursor.argtypes = [c.c_void_p, c.c_ulong]
xlib.XCloseDisplay.argtypes = [c.c_void_p]
cursor.XcursorImageCreate.argtypes = [c.c_int, c.c_int]
cursor.XcursorImageCreate.restype = c.POINTER(Image)
cursor.XcursorImageLoadCursor.argtypes = [c.c_void_p, c.POINTER(Image)]
cursor.XcursorImageLoadCursor.restype = c.c_ulong
cursor.XcursorImageDestroy.argtypes = [c.POINTER(Image)]
display = xlib.XOpenDisplay(None)
if not display:
    raise RuntimeError('The private XWayland display is unavailable')
image = cursor.XcursorImageCreate(96, 96)
try:
    image.contents.xhot = 0
    image.contents.yhot = 0
    for i in range(96 * 96):
        image.contents.pixels[i] = 0xffffffff
    handle = cursor.XcursorImageLoadCursor(display, image)
    xlib.XDefineCursor(display, xlib.XDefaultRootWindow(display), handle)
    xlib.XFreeCursor(display, handle)
finally:
    cursor.XcursorImageDestroy(image)
    xlib.XCloseDisplay(display)
