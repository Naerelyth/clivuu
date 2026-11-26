"""Private-session regression: inspect backing before GTK can repaint a remap."""
import ctypes as c
import hashlib
import json
import os
import signal
import subprocess
import sys

x = c.CDLL('libX11.so.6')
composite = c.CDLL('libXcomposite.so.1')
p, w = c.c_void_p, c.c_ulong
x.XOpenDisplay.argtypes = [c.c_char_p]; x.XOpenDisplay.restype = p
x.XDefaultRootWindow.argtypes = [p]; x.XDefaultRootWindow.restype = w
x.XQueryTree.argtypes = [p,w,c.POINTER(w),c.POINTER(w),c.POINTER(c.POINTER(w)),c.POINTER(c.c_uint)]
x.XFetchName.argtypes = [p,w,c.POINTER(c.c_char_p)]
x.XGetGeometry.argtypes = [p,w,c.POINTER(w),c.POINTER(c.c_int),c.POINTER(c.c_int),c.POINTER(c.c_uint),c.POINTER(c.c_uint),c.POINTER(c.c_uint),c.POINTER(c.c_uint)]
x.XFree.argtypes = [p]; x.XCloseDisplay.argtypes = [p]
x.XGetImage.argtypes = [p,w,c.c_int,c.c_int,c.c_uint,c.c_uint,w,c.c_int]; x.XGetImage.restype = p
x.XDestroyImage.argtypes = [p]; x.XFreePixmap.argtypes = [p,w]
composite.XCompositeNameWindowPixmap.argtypes = [p,w]; composite.XCompositeNameWindowPixmap.restype = w

class Image(c.Structure):
    _fields_ = [('width',c.c_int),('height',c.c_int),('xoffset',c.c_int),('format',c.c_int),
                ('data',p),('byte_order',c.c_int),('bitmap_unit',c.c_int),('bitmap_bit_order',c.c_int),
                ('bitmap_pad',c.c_int),('depth',c.c_int),('bytes_per_line',c.c_int),('bits_per_pixel',c.c_int)]

display = x.XOpenDisplay(None)
if not display: raise RuntimeError('Private X display is unavailable')
pid = int(sys.argv[2])
# Only the explicitly supplied private D-Bus service is stopped. Always resume
# it even if pixel comparison or the worker fails. The user's app is untouched.
stopped = False
try:
    os.kill(pid,signal.SIGSTOP); stopped = True
    root,parent,children,count = w(),w(),c.POINTER(w)(),c.c_uint()
    x.XQueryTree(display,x.XDefaultRootWindow(display),c.byref(root),c.byref(parent),c.byref(children),c.byref(count))
    hosts=[]
    for xid in children[:count.value]:
        title=c.c_char_p()
        if x.XFetchName(display,xid,c.byref(title)):
            if title.value == b'Clivuu pins': hosts.append(xid)
            x.XFree(title)
    if children: x.XFree(children)
    if len(hosts)!=1: raise RuntimeError('Expected one private pin host')
    xid=hosts[0]
    xx,yy,width,height,border,depth=c.c_int(),c.c_int(),c.c_uint(),c.c_uint(),c.c_uint(),c.c_uint()
    x.XGetGeometry(display,xid,c.byref(root),c.byref(xx),c.byref(yy),c.byref(width),c.byref(height),c.byref(border),c.byref(depth))
    def pixels():
        pixmap=composite.XCompositeNameWindowPixmap(display,xid)
        try:
            raw=x.XGetImage(display,pixmap,0,0,width.value,height.value,w(-1).value,2)
            if not raw: raise RuntimeError('Cannot read pin backing')
            try:
                image=c.cast(raw,c.POINTER(Image)).contents
                return c.string_at(image.data,image.bytes_per_line*image.height)
            finally: x.XDestroyImage(raw)
        finally: x.XFreePixmap(display,pixmap)
    before=pixels()
    subprocess.run([sys.argv[1],'worker','x11-release-focus',str(xid),str(xx.value),str(yy.value),
                    str(width.value),str(height.value)],check=True,timeout=10)
    after=pixels()
    print(json.dumps(dict(preserved=before==after,bytes=len(before),
                          changed_bytes=sum(a!=b for a,b in zip(before,after)),
                          before=hashlib.sha256(before).hexdigest(),after=hashlib.sha256(after).hexdigest())))
finally:
    if stopped: os.kill(pid,signal.SIGCONT)
    x.XCloseDisplay(display)
