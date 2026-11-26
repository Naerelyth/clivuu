"""Read a reference application's X11 cursor in the private test session.

No Xfixes development headers, input grabs or product capture code are used.
"""
import ctypes as c
import json
from pathlib import Path
import sys

import cairo


class CursorImage(c.Structure):
    _fields_ = [
        ('x', c.c_short), ('y', c.c_short),
        ('width', c.c_ushort), ('height', c.c_ushort),
        ('xhot', c.c_ushort), ('yhot', c.c_ushort),
        ('serial', c.c_ulong), ('pixels', c.POINTER(c.c_ulong)),
        ('atom', c.c_ulong), ('name', c.c_char_p),
    ]


xlib = c.CDLL('libX11.so.6')
fixes = c.CDLL('libXfixes.so.3')
xlib.XOpenDisplay.restype = c.c_void_p
xlib.XOpenDisplay.argtypes = [c.c_char_p]
xlib.XFree.argtypes = [c.c_void_p]
xlib.XCloseDisplay.argtypes = [c.c_void_p]
fixes.XFixesGetCursorImage.restype = c.POINTER(CursorImage)
fixes.XFixesGetCursorImage.argtypes = [c.c_void_p]

display = xlib.XOpenDisplay(None)
if not display:
    raise RuntimeError('No private reference X11 display')
pointer = None
try:
    pointer = fixes.XFixesGetCursorImage(display)
    if not pointer:
        raise RuntimeError('No reference cursor image')
    image = pointer.contents
    if not (0 < image.width <= 1024 and 0 < image.height <= 1024 and image.pixels):
        raise RuntimeError('Invalid reference cursor dimensions')
    pixels = [image.pixels[n] & 0xffffffff for n in range(image.width * image.height)]
    output = Path(sys.argv[1])
    output.with_suffix('.json').write_text(json.dumps({
        'width': image.width, 'height': image.height,
        'hotspot': [image.xhot, image.yhot], 'point': [image.x, image.y],
        'pixels': pixels,
    }))
    surface = cairo.ImageSurface(cairo.FORMAT_ARGB32, image.width, image.height)
    data = surface.get_data()
    for y in range(image.height):
        for x in range(image.width):
            offset = y * surface.get_stride() + x * 4
            data[offset:offset + 4] = pixels[y * image.width + x].to_bytes(4, sys.byteorder)
    surface.mark_dirty()
    surface.write_to_png(str(output.with_suffix('.png')))
finally:
    if pointer:
        xlib.XFree(pointer)
    xlib.XCloseDisplay(display)
