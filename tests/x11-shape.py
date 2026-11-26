"""Read X server shapes; optional private-test grab probe releases immediately."""
import ctypes as c
import json
import sys

x = c.CDLL('libX11.so.6')
shape = c.CDLL('libXext.so.6')
ptr = c.c_void_p
win = c.c_ulong
x.XOpenDisplay.argtypes = [c.c_char_p]; x.XOpenDisplay.restype = ptr
x.XDefaultRootWindow.argtypes = [ptr]; x.XDefaultRootWindow.restype = win
x.XQueryTree.argtypes = [ptr,win,c.POINTER(win),c.POINTER(win),c.POINTER(c.POINTER(win)),c.POINTER(c.c_uint)]
x.XFetchName.argtypes = [ptr,win,c.POINTER(c.c_char_p)]
x.XGetGeometry.argtypes = [ptr,win,c.POINTER(win),c.POINTER(c.c_int),c.POINTER(c.c_int),c.POINTER(c.c_uint),c.POINTER(c.c_uint),c.POINTER(c.c_uint),c.POINTER(c.c_uint)]
x.XFree.argtypes = [ptr]; x.XCloseDisplay.argtypes = [ptr]
read_gutter = '--gutter' in sys.argv
probe_grab = '--grab-status' in sys.argv
if probe_grab:
    # Isolated tests only. An unsuccessful probe cannot steal an existing grab;
    # a successful probe is immediately released. No production use.
    x.XGrabPointer.argtypes=[ptr,win,c.c_int,c.c_uint,c.c_int,c.c_int,win,win,win]
    x.XUngrabPointer.argtypes=[ptr,win];x.XSync.argtypes=[ptr,c.c_int]
if read_gutter:
    composite=c.CDLL('libXcomposite.so.1')
    composite.XCompositeNameWindowPixmap.argtypes=[ptr,win];composite.XCompositeNameWindowPixmap.restype=win
    x.XGetImage.argtypes=[ptr,win,c.c_int,c.c_int,c.c_uint,c.c_uint,c.c_ulong,c.c_int];x.XGetImage.restype=ptr
    x.XGetPixel.argtypes=[ptr,c.c_int,c.c_int];x.XGetPixel.restype=c.c_ulong
    x.XDestroyImage.argtypes=[ptr];x.XFreePixmap.argtypes=[ptr,win]
x.XQueryPointer.argtypes=[ptr,win,c.POINTER(win),c.POINTER(win),c.POINTER(c.c_int),c.POINTER(c.c_int),c.POINTER(c.c_int),c.POINTER(c.c_int),c.POINTER(c.c_uint)]

class Rect(c.Structure):
    _fields_ = [('x',c.c_short),('y',c.c_short),('width',c.c_ushort),('height',c.c_ushort)]

shape.XShapeGetRectangles.argtypes = [ptr,win,c.c_int,c.POINTER(c.c_int),c.POINTER(c.c_int)]
shape.XShapeGetRectangles.restype = c.POINTER(Rect)
display = x.XOpenDisplay(None)
if not display: raise RuntimeError('Private X display is unavailable')
try:
    root, parent, children, count = win(),win(),c.POINTER(win)(),c.c_uint()
    x.XQueryTree(display,x.XDefaultRootWindow(display),c.byref(root),c.byref(parent),c.byref(children),c.byref(count))
    result=[]
    for i in range(count.value):
        title=c.c_char_p(); xid=children[i]
        if not x.XFetchName(display,xid,c.byref(title)):continue
        name=title.value.decode(errors='replace');x.XFree(title)
        if name!='Clivuu pins':continue
        px,py,w,h,border,depth=c.c_int(),c.c_int(),c.c_uint(),c.c_uint(),c.c_uint(),c.c_uint()
        x.XGetGeometry(display,xid,c.byref(root),c.byref(px),c.byref(py),c.byref(w),c.byref(h),c.byref(border),c.byref(depth))
        n,order=c.c_int(),c.c_int();rects=shape.XShapeGetRectangles(display,xid,2,c.byref(n),c.byref(order))
        rr,child=win(),win();rx,ry,wx,wy=c.c_int(),c.c_int(),c.c_int(),c.c_int();mask=c.c_uint()
        x.XQueryPointer(display,x.XDefaultRootWindow(display),c.byref(rr),c.byref(child),c.byref(rx),c.byref(ry),c.byref(wx),c.byref(wy),c.byref(mask))
        result.append({'xid':xid,'geometry':dict(x=px.value,y=py.value,width=w.value,height=h.value),
                       'pointer':dict(x=rx.value,y=ry.value,child=child.value,mask=mask.value),
                       'input':[dict(x=r.x,y=r.y,width=r.width,height=r.height) for r in rects[:n.value]]})
        if probe_grab:
            status=x.XGrabPointer(display,x.XDefaultRootWindow(display),0,0,1,1,0,0,0)
            result[-1]['grab_status']=status
            if status==0:x.XUngrabPointer(display,0)
            x.XSync(display,0)
        if rects:x.XFree(rects)
        bound=shape.XShapeGetRectangles(display,xid,0,c.byref(n),c.byref(order))
        result[-1]['bounding']=[dict(x=r.x,y=r.y,width=r.width,height=r.height) for r in bound[:n.value]]
        if bound:x.XFree(bound)
        if read_gutter:
            pixmap=composite.XCompositeNameWindowPixmap(display,xid)
            try:
                samples={}
                for label,xx,yy,ww,hh in [('right',w.value-1,0,1,h.value),('bottom',0,h.value-1,w.value,1)]:
                    image=x.XGetImage(display,pixmap,xx,yy,ww,hh,c.c_ulong(-1).value,2)
                    if not image:raise RuntimeError('Cannot read named pin pixmap')
                    try:samples[label]=sum(x.XGetPixel(image,px,py)!=0 for py in range(hh) for px in range(ww))
                    finally:x.XDestroyImage(image)
                result[-1]['gutter_nontransparent']=samples
            finally:x.XFreePixmap(display,pixmap)
    if children:x.XFree(children)
    print(json.dumps(result))
finally:x.XCloseDisplay(display)
