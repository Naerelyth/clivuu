import {historyShortcut} from '../common/annotation-history.js';
import {AnnotationEditor, DRAW_TOOLS,PRIMARY_TOOLS,TOOL_SHORTCUTS,markHandles,isCompleteMark,isFreehand} from '../common/annotation-model.js';
import {drawAnnotations,drawElementSelection,drawElementHover,drawDraftBounds,measureText,clearMosaicCache} from '../common/annotation-render.js';
import {annotationOptions} from './annotation-options.js';
import {InlineText} from './inline-text.js';
import {observeToolbarTheme} from '../common/appearance.js';
import {applyGtkTheme} from '../common/gtk-theme.js';
import {TOOLBAR_HEIGHT,TOOLBAR_THEMES,toolbarControlCss} from '../common/toolbar-theme.js';
import {toolbarGroup,toolbarActions} from '../common/toolbar-model.js';
import Gtk from 'gi://Gtk?version=4.0';
import Gdk from 'gi://Gdk?version=4.0';
import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import Cairo from 'cairo';
import Pango from 'gi://Pango';
import PangoCairo from 'gi://PangoCairo';
import {t} from '../common/i18n.js';
import {bounds, clamp, contains, fromPoints, intersect, monitorAt, selectionPanelLayout, annotationPanelPosition, textureRect, magnifierPixel, magnifierSelectionRect, selectionResizeEdge, pinResizeCursor, resizeSelection, selectionPoint} from '../common/geometry.js';
import {captureOptions, showCaptureMagnifier, observeCaptureAppearance, drawMagnifierCrosshair, drawCaptureFrame, drawCaptureMask, drawCaptureGuides, historyArea, captureLabel} from '../common/capture-options.js';
import {DesktopOverlay} from './overlay.js';
import {drawToolIcon} from './tool-icon.js';
import {CaptureCanvas,snapshotCapture} from './capture-canvas.js';

// A frozen, aspect-correct desktop canvas. The canvas can span several sources
// with independent DPRs; pointer coordinates are always compositor coordinates.
// Captures use a desktop overlay. Pin editing reuses the existing pin surface.
// This class never constructs a managed image-editor window.
export class Selection {
    constructor(path, monitors, windows = [], rect = null, options = {}) {
        this.monitors = monitors;
        this.capture = options.capture ?? captureOptions();
        this.appearanceSettings = options.settings;
        this.historyAreas = options.historyAreas ?? []; this.historyIndex = -1;
        this.windows = windows;
        this.limit = bounds(monitors);
        this.rect = rect ?? {...this.limit};
        this.selected = Boolean(rect);
        this.level = 0;
        this.tool = 'select';
        this.color = '#ff6058';
        
        this.defaultAction = options.defaultAction ?? 'clipboard';
        this.embedded = options.embedded;
        this.previewPin = options.previewPin;
        this.overlayOptions = options.overlay ?? {binary: Gio.File.new_for_uri(import.meta.url).get_parent().get_parent().get_child('bin/clivuu').get_path()};
        this.image = Cairo.ImageSurface.createFromPNG(path);
        const unit=this.image.getWidth()/this.limit.width;
        const strokeScale=this.embedded?this.image.getWidth()/this.embedded.pins._rect(this.embedded.pin).width:unit;
        this.editor=new AnnotationEditor(unit,measureText,strokeScale);
        this.history=this.editor.history; this.marks=this.editor.marks; this.redo=this.history.redo; this.mosaicCache=new Map();
        if(!this.embedded)this.texture=Gdk.Texture.new_for_pixbuf(Gdk.pixbuf_get_from_surface(this.image,0,0,this.image.getWidth(),this.image.getHeight()));
        this.window = null; this.controls = new Map();this.toolbarGroups=new WeakMap();
        const column = new Gtk.Box({orientation: Gtk.Orientation.VERTICAL, spacing: 6});
        this.hint = new Gtk.Label({xalign: 0, ellipsize: Pango.EllipsizeMode.END});
        const tools = new Gtk.Box({spacing: 2,valign:Gtk.Align.CENTER});
        this.grip = this._button(tools, t("Move toolbar"), () => {}, 'grip');
        this.buttons = new Map();
        for (const [kind, label] of PRIMARY_TOOLS) {
            const button = this._button(tools, t(label), () => this._tool(kind==='rect'?(this.shapeTool??'rect'):kind), kind,kind==='rect'?'shape':kind); this.buttons.set(kind, button);
        }
        this._button(tools, t("Undo"), () => this._undo(), 'undo'); this._button(tools, t("Redo"), () => this._redo(), 'redo');
        if (this.embedded) this.buttons.set('crop', this._button(tools, t("Crop"), () => this._tool('crop'), 'crop'));
        for (const [kind, label] of options.extraActions ?? [])
            this._button(tools, t(label), () => this.finish(kind), kind);
        const scroller = new Gtk.ScrolledWindow({hscrollbar_policy: Gtk.PolicyType.AUTOMATIC,
            vscrollbar_policy: Gtk.PolicyType.NEVER, child: tools,vexpand:true}); column.append(scroller);
        this.toolScroller = scroller;
        this.options=annotationOptions(this);
        const outputs = tools;
        for (const [action, label] of toolbarActions(options.actions ?? [['clipboard', t("Copy ↵")], ['pin', t("Pin P")], ['quick-save', t("Save S")],
            ['ocr_clipboard', t("OCR O")], ['scroll', t("Scroll L")], ['record', t("Record V")], [null, t("Cancel Esc")]]))
            this._button(outputs, label, () => this.finish(action), action ?? 'cancel');
        this.toolbar = column;
        this._buildOverlay();
        this.inlineText=new InlineText(this);
        this._update();
    }

    get selectingImage() { return !this.embedded || this.tool === 'crop'; }

    _input(drawing, point) {
        let pointerActive=false;
        const endPointer=()=>{
            if(!pointerActive)return;pointerActive=false;
            if(this.editor.edit)this.editor.end();else if(isCompleteMark(this.mark))this.editor.add(this.mark);
            this.mark=null;this.drag=null;this.selected=true;this._update();
        };
        const click = new Gtk.GestureClick({button: 0});
        click.connect('pressed', (g, count, x, y) => {
            if (g.get_current_button() === 3) {
                this._commitText();
                if(this.editor.active>=0){this.editor.select(-1);this._update();g.set_state(Gtk.EventSequenceState.CLAIMED);return;}
                if (this.embedded) {
                    const r = this.embedded.pins._rect(this.embedded.pin), [px, py] = point(x, y);
                    this.embedded.pins.openMenuAt(this.embedded.pin, r.x + px * r.width / this.limit.width,
                        r.y + py * r.height / this.limit.height);
                    g.set_state(Gtk.EventSequenceState.CLAIMED); return;
                }
                if (this.selected) { this.selected = false; this.editor.clear(); this._update(); }
                else this.finish(null);
            } else if(count===2&&g.get_current_button()===1&&this.tool==='select'){
                pointerActive=false;const p=point(x,y),index=this.editor.hit(p);
                if(this.editor.marks[index]?.kind==='text'){this.editor.edit=null;this.inlineText.begin(p,index);g.set_state(Gtk.EventSequenceState.CLAIMED);}
                else this.finish(this.defaultAction);
            } else if(g.get_current_button()===1){pointerActive=true;this._press(point(x,y));this._positionMagnifier();this._cursor(drawing);}
        }); drawing.add_controller(click);
        const motion = new Gtk.EventControllerMotion();
        const trackPointer = (_c, x, y) => {
            this.pointer = point(x, y);
            if (!this.selected && !this.drag && !this.mark) this._hover(this.pointer);
            else if (this.overlay) this._queueUpdate();
            this._cursor(drawing);
        };
        motion.connect('motion', trackPointer); motion.connect('enter', trackPointer); drawing.add_controller(motion);
        click.connect('released',g=>{if(g.get_current_button()===1)endPointer();});
        const drag = new Gtk.GestureDrag({button: 1});
        drag.connect('drag-update', (g, dx, dy) => {
            if(!pointerActive)return;const [, x, y] = g.get_start_point(); this._motion(point(x + dx, y + dy), Boolean(g.get_current_event_state() & Gdk.ModifierType.SHIFT_MASK));
        });
        drag.connect('drag-end',endPointer);drawing.add_controller(drag);click.group(drag);
        const scroll = new Gtk.EventControllerScroll({flags: Gtk.EventControllerScrollFlags.VERTICAL});
        scroll.connect('scroll', (_c, _dx, dy) => this._scroll(dy)); drawing.add_controller(scroll);
    }

    _scroll(dy) {
        const kind=this.editor.current?.kind??this.tool;
        if(this.selected&&DRAW_TOOLS.includes(kind)){this.editor.wheel(kind,dy);this._update();return true;}
        if (!this.selected) { this.level = Math.max(0, this.level + Math.sign(dy)); this._hover(this.pointer ?? [this.rect.x, this.rect.y]); }
        return true;
    }

    _button(row, label, action, kind,iconKind=kind) {
        const group=toolbarGroup(kind),previous=this.toolbarGroups.get(row);
        if(group&&previous&&group!==previous){const separator=new Gtk.Separator({orientation:Gtk.Orientation.VERTICAL,margin_start:4,margin_end:4,margin_top:3,margin_bottom:3,can_target:false});separator.add_css_class('clivuu-tool-separator');row.append(separator);}
        if(group)this.toolbarGroups.set(row,group);
        const button = new Gtk.Button({tooltip_text: captureLabel(label, this.capture.tips), focusable: false});
        const icon = new Gtk.DrawingArea({content_width: 18, content_height: 18});
        icon.set_draw_func((_a, ctx, w, h) => drawToolIcon(ctx, w, h, iconKind, button.has_css_class('suggested-action')?(this.toolbarSelectedInk??'#282a36'):(this.toolbarInk??'#404040')));
        button.set_child(icon);
        button.connect('clicked', action);
        row.append(button); this.controls.set(kind, button); return button;
    }

    _cursor(drawing) {
        let cursor = 'crosshair';
        const annotation=this.selected&&this.tool!=='crop'&&this.pointer&&!this.mark&&!this.drag
            ?this.editor.cursor(this.pointer,this.tool,this.scale??1):null;
        if (this.embedded && this.tool === 'select'&&!this.editor.current&&!annotation) { drawing.set_cursor(null); return; } // Pin manager owns move/resize.
        if(annotation)cursor=annotation;
        else if (this.drag?.kind === 'resize') {
            const [w, n, e, s] = this.drag.edge;
            cursor = pinResizeCursor((n ? 'n' : s ? 's' : '') + (w ? 'w' : e ? 'e' : ''));
        } else if (this.drag?.kind === 'move') cursor = 'move';
        else if (this.selected && this.pointer && !this.drag && !this.mark) {
            const annotation = !['select', 'crop'].includes(this.tool) && contains(this.rect, ...this.pointer);
            const edge = annotation || !this.selectingImage ? '' : selectionResizeEdge(this.rect, ...this.pointer, 8 / this.scale);
            if (edge) cursor = pinResizeCursor(edge);
            else if (this.tool === 'select' && contains(this.rect, ...this.pointer)) cursor = 'move';
        }
        drawing.set_cursor_from_name(cursor);
    }

    _buildOverlay() {
        this.scale = 1;
        this.toolbar.add_css_class('clivuu-capture-toolbar');
        this.toolbar.set_cursor_from_name('default');
        this.options.widget.add_css_class('clivuu-capture-toolbar');
        this.options.widget.set_cursor_from_name('default');
        this.hint.add_css_class('clivuu-capture-hint');
        this.hint.can_target = false;
        if (!this.embedded) {
            this.magnifier = new Gtk.DrawingArea({content_width: 184, content_height: 236, can_target: false});
            this.magnifier.set_draw_func((_a, ctx, w, h) => this._drawMagnifier(ctx, w, h));
        }
        this.provider = new Gtk.CssProvider();
        const dark=TOOLBAR_THEMES.dark;
        this.provider.load_from_string(`
            .clivuu-capture-toolbar { background:#e6e6e6; color:#404040; border-radius:5px; padding:3px; border:1px solid #999; box-shadow:0 3px 8px #0005; font-size:12px; }
            .clivuu-capture-toolbar button:not(.clivuu-color-action) { padding:3px; min-width:18px; min-height:18px; border:0; background:transparent; color:#404040; box-shadow:none; border-radius:2px; }
            .clivuu-capture-toolbar button:not(.clivuu-color-action):hover { background:#d1d1d1; }
            .clivuu-capture-toolbar button.suggested-action:not(.clivuu-color-action), .clivuu-capture-toolbar button:checked { background:#ffb500; }
            .clivuu-annotation-options button.clivuu-option { padding:0; min-width:24px; min-height:24px; border-radius:0; }
            .clivuu-annotation-options menubutton.clivuu-option > button { padding:0 2px; min-height:24px; border-radius:0; }
            .clivuu-annotation-options menubutton.clivuu-current-color > button { min-width:22px; min-height:22px; padding:1px; }
            .clivuu-annotation-options button.clivuu-swatch { padding:0; min-width:10px; min-height:10px; border:1px solid #8a8a8a; border-radius:0; }
            .clivuu-annotation-options button.clivuu-swatch-selected { border-color:#404040; box-shadow:inset 0 0 0 1px white; }
            .clivuu-annotation-options separator { background:#9b9b9b; min-width:1px; }
            .clivuu-tool-separator { background:#9b9b9b; min-width:1px; }
            .clivuu-annotation-options scale { min-height:18px; padding:0 6px; }
            .clivuu-color-editor { font-size:12px; }
            .clivuu-color-editor spinbutton, .clivuu-color-editor entry { min-height:24px; border-radius:3px; }
            .clivuu-color-editor spinbutton text { padding:0 3px; min-width:24px; }
            .clivuu-color-editor spinbutton button { min-width:16px; padding:1px; }
            .clivuu-color-editor entry { padding:1px 5px; }
            .clivuu-color-editor .clivuu-color-action { min-width:56px; min-height:24px; padding:4px 12px; font-weight:600; }
            .clivuu-capture-toolbar.clivuu-theme-dark { background:${dark.background}; color:${dark.ink}; border-color:${dark.border}; }
            .clivuu-capture-toolbar.clivuu-theme-dark button:not(.clivuu-color-action) { color:${dark.ink}; }
            .clivuu-capture-toolbar.clivuu-theme-dark button:not(.clivuu-color-action):hover { background:${dark.hover}; }
            .clivuu-capture-toolbar.clivuu-theme-dark button.suggested-action:not(.clivuu-color-action), .clivuu-capture-toolbar.clivuu-theme-dark button:checked { background:${dark.selected}; color:${dark.selectedInk}; }
            .clivuu-theme-dark .clivuu-tool-separator, .clivuu-theme-dark .clivuu-annotation-options separator { background:${dark.divider}; }
            .clivuu-theme-dark.clivuu-capture-toolbar separator { background:${dark.divider}; }
            .clivuu-theme-dark button.clivuu-swatch-selected { border-color:white; box-shadow:inset 0 0 0 1px ${dark.background}; }
            .clivuu-capture-hint { color: white; background: #161b22ed; border-radius: 5px; padding: 6px 9px; font-size: 12px; }
        `);
        Gtk.StyleContext.add_provider_for_display(Gdk.Display.get_default(), this.provider, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION);
        try { this.overlay = this.embedded ? this.embedded.pins.embed(this, this.embedded.pin) : new DesktopOverlay(this.monitors, this.overlayOptions.binary, view => {
            const drawing = new CaptureCanvas();
            drawing.render=snapshot=>snapshotCapture(snapshot,this,view.rect);
            this._input(drawing, (x, y) => [clamp(x + view.rect.x, this.limit.x, this.limit.x + this.limit.width),
                clamp(y + view.rect.y, this.limit.y, this.limit.y + this.limit.height)]);
            return drawing;
        }, (symbol, state, window) => { this.window = window; return this._key(symbol, state); }, error => {
            this.failure = error; this.finish(null);
        }); } catch (error) {
            Gtk.StyleContext.remove_provider_for_display(Gdk.Display.get_default(), this.provider);
            this.image.finish(); throw error;
        }
        this.window = this.overlay.views[0].window;
        const grip = new Gtk.GestureDrag({button:1,propagation_phase:Gtk.PropagationPhase.CAPTURE});
        grip.connect('drag-begin', g => {
            const [,x,y]=g.get_current_event().get_position();
            this.toolbarStart={...this.toolbarPosition,pointer:[x,y]};g.set_state(Gtk.EventSequenceState.CLAIMED);
        });
        const move=g=>{
            if(!this.toolbarStart)return;
            const [,x,y]=g.get_current_event().get_position(),m=this.toolbarMonitor,p=this.toolbarStart;
            this.toolbarCustom={x:clamp(p.x+x-p.pointer[0],m.x+1,m.x+m.width-p.width-1),
                y:clamp(p.y+y-p.pointer[1],m.y+1,m.y+m.height-p.height-1)};
            if(this.toolbarTick)return;
            this.toolbarTick=this.grip.add_tick_callback(()=>{this.toolbarTick=0;this._positionUI();
                for(const v of this.overlay.views)if(v.surface)this.embedded.pins._inputRegion(v.surface);
                return GLib.SOURCE_REMOVE;});
        };
        grip.connect('drag-update',move);grip.connect('drag-end',g=>{move(g);this.toolbarStart=null;});this.grip.add_controller(grip);
    }

    _positionUI() {
        if (!this.overlay?.views.length) return;
        const r = this.embedded ? this.embedded.pins._rect(this.embedded.pin) : this.rect;
        const monitors = this.embedded ? this.embedded.pins.geometry() : this.monitors;
        const m = monitorAt(monitors, r.x + r.width / 2, r.y + r.height / 2);
        const view = this.overlay.views.find(v => contains(v.rect, m.x + m.width / 2, m.y + m.height / 2)) ?? this.overlay.views[0];
        for (const child of [this.toolbar, this.hint, this.options.widget]) {
            const parent = child.get_parent();
            if (parent !== view.fixed) { parent?.remove(child); view.fixed.put(child, 0, 0); }
        }
        // GTK reports zero requisition for an invisible widget. Make the panel
        // measurable before placing it after a drag, otherwise its first frame
        // can extend past the output's right edge.
        const visible = this.selected && !this.drag && !this.mark && (!this.editor.edit||this.editor.current?.kind==='text') && !this.embedded?.pins.hidden;
        this.hint.visible = this.selectingImage && !(this.capture.hide1x1 && !this.selected && this.rect.width === 1 && this.rect.height === 1);
        this.options.widget.set_max_content_width(Math.max(1,m.width-10));
        const metricKey=`${m.width}:${this.editor.current?.kind??this.tool}`;
        if(this.toolbarMetrics?.key!==metricKey){
            this.toolbar.visible=true;
            this.toolScroller.set_min_content_width(0);
            this.toolScroller.set_max_content_width(Math.min(1000, m.width - 24));
            this.toolScroller.set_propagate_natural_width(true);
            this.toolbar.set_size_request(-1, -1);
            const [,naturalWidth]=this.toolbar.measure(Gtk.Orientation.HORIZONTAL,-1);
            const width=Math.min(m.width-2,naturalWidth);
            this.toolbar.set_size_request(width,TOOLBAR_HEIGHT);
            const height=TOOLBAR_HEIGHT;
            this.toolbarMetrics={key:metricKey,width,height};
        }
        this.toolbar.visible=visible;
        const {width,height}=this.toolbarMetrics;
        const [labelWidth] = this.hint.create_pango_layout(this.hint.label).get_pixel_size();
        const hintWidth = Math.min(m.width - 2, labelWidth + 20);
        this.hint.set_size_request(hintWidth, -1);
        const [, hintHeight] = this.hint.measure(Gtk.Orientation.VERTICAL, hintWidth);
        const layout = selectionPanelLayout(r, m, {width, height}, {width: hintWidth, height: hintHeight});
        const position = this.toolbarCustom ? {...layout.toolbar,
            x: clamp(this.toolbarCustom.x, m.x + 1, m.x + m.width - width - 1),
            y: clamp(this.toolbarCustom.y, m.y + 1, m.y + m.height - height - 1)} : layout.toolbar;
        this.toolbarPosition = position; this.toolbarMonitor = m;
        this.toolbarPlacement = this.toolbarCustom ? 'custom' : layout.placement;
        view.fixed.move(this.toolbar, position.x - view.rect.x, position.y - view.rect.y);
        view.fixed.move(this.hint, layout.hint.x - view.rect.x, layout.hint.y - view.rect.y);
        const panel=this.options.widget;panel.visible=visible&&this.options.active();
        this.optionsPosition=null;
        if(panel.visible){
            panel.set_size_request(-1,-1);
            const [,natural]=panel.measure(Gtk.Orientation.HORIZONTAL,-1),width=Math.min(m.width-2,natural);
            const p=annotationPanelPosition(position,m,{width,height:TOOLBAR_HEIGHT});
            panel.set_size_request(p.width,TOOLBAR_HEIGHT);view.fixed.move(panel,p.x-view.rect.x,p.y-view.rect.y);this.optionsPosition=p;
        }
        this._positionMagnifier();
        this.inlineText?.layout();this.options.reposition?.();
    }

    _positionMagnifier() {
        if (!this.magnifier || !this.pointer) return;
        this.magnifier.visible = showCaptureMagnifier(this.selected, this.drag);
        if (!this.magnifier.visible) return;
        const [px, py] = this.pointer, m = monitorAt(this.monitors, px, py), width = 184, height = 236;
        const view = this.overlay.views.find(v => contains(v.rect, px, py)) ?? this.overlay.views[0];
        if (this.magnifier.get_parent() !== view.fixed) {
            this.magnifier.get_parent()?.remove(this.magnifier); view.fixed.put(this.magnifier, 0, 0);
        }
        const avoid = this.toolbar.visible ? this.toolbarPosition : null;
        const candidates = [[px + 26, py + 26], [px - width - 26, py + 26],
            [px + 26, py - height - 26], [px - width - 26, py - height - 26]].map(([x, y]) => ({
            x: clamp(x, m.x + 8, m.x + m.width - width - 8),
            y: clamp(y, m.y + 8, m.y + m.height - height - 8), width, height}));
        const position = candidates.find(r => !contains(r, px, py) && (!avoid || !intersect(r, avoid).width || !intersect(r, avoid).height)) ?? candidates[0];
        view.fixed.move(this.magnifier, position.x - view.rect.x, position.y - view.rect.y);
        this.magnifier.queue_draw();
    }

    _pixel() {
        const [px, py] = magnifierPixel(this.pointer ?? [this.rect.x, this.rect.y], this.rect, this.limit, this.image.getWidth(), this.image.getHeight(), this.drag);
        // Extract just one pixel from the existing Cairo surface; never keep a
        // second full-size image merely to display the colour under the cursor.
        const color = Array.from(Gdk.pixbuf_get_from_surface(this.image, px, py, 1, 1).get_pixels()).slice(0, 3);
        return {px, py, color, value: this.hexColor ? '#' + color.map(c => c.toString(16).padStart(2, '0')).join('').toUpperCase() : color.join(', ')};
    }

    _drawMagnifier(ctx, width, height) {
        if (!this.pointer) return;
        const {px, py, color, value} = this._pixel(), size = 10, left = px - 8, top = py - 6;
        ctx.setSourceRGBA(0.035, 0.04, 0.05, 0.97); ctx.rectangle(0, 0, width, height); ctx.fill();
        ctx.save(); ctx.rectangle(7, 7, 170, 130); ctx.clip(); ctx.translate(7, 7); ctx.scale(size, size);
        ctx.setSourceSurface(this.image, -left, -top); ctx.getSource().setFilter(Cairo.Filter.NEAREST); ctx.paint();
        const sx = this.image.getWidth() / this.limit.width;
        const pixelRect = magnifierSelectionRect(this.rect, this.limit, this.image.getWidth(), this.image.getHeight(), px, py);
        if (this.capture.magnifierMask) drawCaptureMask(ctx, pixelRect, {x: 0, y: 0, width: 17, height: 13}, this.capture);
        if (this.capture.magnifierBorder) drawCaptureFrame(ctx, pixelRect, {...this.capture, anchors:true, borderWidth: this.capture.borderWidth * sx}, {anchors: this.capture.magnifierAnchors && this.selected});
        else if (this.capture.magnifierAnchors) drawCaptureFrame(ctx, pixelRect, {...this.capture, anchors:true, borderWidth: 0}, {anchors: this.selected});
        // Mask/border/large anchors must not replace the sampled source pixel.
        ctx.save(); ctx.rectangle(8, 6, 1, 1); ctx.clip();
        ctx.setSourceSurface(this.image, -left, -top); ctx.getSource().setFilter(Cairo.Filter.NEAREST); ctx.paint(); ctx.restore();
        drawMagnifierCrosshair(ctx, this.capture);
        ctx.restore();
        ctx.setLineWidth(1); ctx.setSourceRGB(1, 1, 1); ctx.rectangle(6.5, 6.5, 171, 131); ctx.stroke();
        // A contrasting two-stroke square identifies the exact centre pixel.
        ctx.setSourceRGB(0, 0, 0); ctx.rectangle(86.5, 66.5, 11, 11); ctx.stroke();
        ctx.setSourceRGB(1, 1, 1); ctx.rectangle(87.5, 67.5, 9, 9); ctx.stroke();
        const text = (value, y, font = 'Monospace 10') => {
            const layout = PangoCairo.create_layout(ctx); layout.set_font_description(Pango.FontDescription.from_string(font));
            layout.set_text(value, -1); const [w] = layout.get_pixel_size();
            ctx.setSourceRGB(1, 1, 1); ctx.moveTo((width - w) / 2, y); PangoCairo.show_layout(ctx, layout);
        };
        text(`(${Number((this.limit.x + px * this.limit.width / this.image.getWidth()).toFixed(2))}, ${Number((this.limit.y + py * this.limit.height / this.image.getHeight()).toFixed(2))})`, 146);
        ctx.setSourceRGB(...color.map(v => v / 255)); ctx.rectangle(13, 171, 13, 13); ctx.fillPreserve();
        ctx.setSourceRGB(1, 1, 1); ctx.stroke(); text(value, 170);
        if (this.capture.tips) { text(t("C: copy color"), 195, 'Sans 9'); text(t("Shift: RGB / HEX"), 214, 'Sans 9'); }
    }
    _tool(tool) {
        this._commitText();
        // A crop is a draft until Apply. Switching back to annotations cancels
        // that draft instead of leaving an invisible crop-only input region.
        if(this.embedded&&this.tool==='crop'&&tool!=='crop'){
            this.rect={...this.limit};this.selected=true;
        }
        this.mark=null;this.drag=null;this.editor.edit=null;
        this.editor.select(-1); this.tool = tool;
        if(['rect','ellipse'].includes(tool))this.shapeTool=tool;
        this.color=this.editor.style(tool).color;
        this.overlay?.layout?.(); this._update();
    }
    _shape(kind){
        if(!this.editor.changeShape(kind)){this._tool(kind);return;}
        this.shapeTool=this.tool=kind;this.color=this.editor.current?.color??this.editor.style(kind).color;this._update();
    }
    _changeStyle(patch,group=null) {
        const tool=this.editor.current?.kind??this.tool;
        this.editor.change(tool,patch,group);this.color=this.editor.style(tool).color;
        this._update();
    }
    _commitText(){if(!this.editor.textEdit)return;this.inlineText?.flush();this.editor.commitText();this.inlineText?.destroy();this.overlay.views.find(v=>v.window===this.window)?.drawing.grab_focus();}
    _undo() { this._commitText();this.editor.undo(); this._update(); }
    _redo() { this._commitText();this.editor.redo(); this._update(); }
    _point(x, y) {
        return [clamp((x - (this.tx ?? 0)) / (this.scale ?? 1) + this.limit.x, this.limit.x, this.limit.x + this.limit.width),
            clamp((y - (this.ty ?? 0)) / (this.scale ?? 1) + this.limit.y, this.limit.y, this.limit.y + this.limit.height)];
    }
    _hover(point) {
        const options = [this.capture.detectWindows && this.windows.find(w => contains(w, ...point)), monitorAt(this.monitors, ...point), this.limit].filter(Boolean);
        this.rect = {...options[Math.min(this.level, options.length - 1)]}; this._update();
    }
    _press(point) {
        this.inlineText?.flush();
        if(this.editor.textEdit&&this.editor.begin(point,this.tool,this.scale??1)){this.pointer=point;this._update();return;}
        this._commitText();
        this.pointer = point;
        const hitPoint = point;
        point = point.map(Math.round);
        const textIndex=this.editor.hit(hitPoint);
        if(this.tool==='text'&&this.selected&&this.editor.marks[textIndex]?.kind==='text'){
            const target=this.editor.target(hitPoint,'select',this.scale??1);
            if(!target?.handle||target.handle.kind==='move'){this.inlineText.begin(point,textIndex);return;}
        }
        if(this.tool!=='crop'&&this.selected&&this.editor.begin(hitPoint,this.tool,this.scale??1)){this._update();return;}
        // Outside Crop, an embedded image has no capture-selection geometry.
        // The pin host handles moving/resizing the image itself.
        if(this.embedded&&this.tool==='select'){this.editor.select(-1);this._update();return;}
        if(this.embedded&&this.tool!=='crop'&&!contains(this.rect,...hitPoint))return;
        if (this.selected && !['select', 'crop'].includes(this.tool) && contains(this.rect, ...hitPoint)) {
            if (this.tool === 'text') {
                this.inlineText.begin(point);
            } else this.mark = this.editor.create(this.tool,point);
        } else {
            const r = this.rect, margin = 8 / (this.scale ?? 1);
            const hit = this.selected ? selectionResizeEdge(r, ...hitPoint, margin) : '';
            const edge = ['w', 'n', 'e', 's'].map(side => hit.includes(side));
            this.drag = {point, rect: {...r}, edge, kind: edge.some(Boolean) ? 'resize' : this.tool !== 'crop' && this.selected && contains(r, ...hitPoint) ? 'move' : 'new'};
        }
    }
    _motion(point, shift = false) {
        this.pointer = point;
        if (this.editor.edit) { this.editor.move(point, shift); this._queueUpdate(); return; }
        point = point.map(Math.round);
        if (this.mark) {
            const p = [clamp(point[0], this.rect.x, this.rect.x + this.rect.width), clamp(point[1], this.rect.y, this.rect.y + this.rect.height)];
            if (isFreehand(this.mark)) { if (this.mark.points.length < 20000) this.mark.points.push(p); }
            else this.mark.points[1] = p;
        } else if (this.drag) {
            const {rect: r, point: p, kind} = this.drag;
            const dx = point[0] - p[0], dy = point[1] - p[1];
            if (kind === 'move') this.rect = {...r, x: clamp(r.x + dx, this.limit.x, this.limit.x + this.limit.width - r.width),
                y: clamp(r.y + dy, this.limit.y, this.limit.y + this.limit.height - r.height)};
            else if (kind === 'resize') this.rect = resizeSelection(this.drag, this.pointer, this.monitors, this.limit);
            else if (Math.abs(dx) + Math.abs(dy) > 4) this.rect = fromPoints(selectionPoint(p, this.monitors), selectionPoint(this.pointer, this.monitors), this.limit);
        }
        this._queueUpdate();
    }
    _key(key, state) {
        if(this.options?.handleKey?.(key))return true;
        if(this.options?.handlesKey())return false;
        const focus = this.window?.get_focus();
        const ctrl = state & Gdk.ModifierType.CONTROL_MASK, shift = state & Gdk.ModifierType.SHIFT_MASK;
        if(this.editor.textEdit&&this.inlineText.containsFocus(focus)){
            if(key===Gdk.KEY_Escape){this._commitText();this.editor.select(-1);this.overlay.views.find(v=>v.window===this.window)?.drawing.grab_focus();this._update();return true;}
            const history=historyShortcut(key,ctrl,shift);
            if(history){this.inlineText.history(history==='redo');return true;}
            if(ctrl&&[Gdk.KEY_Return,Gdk.KEY_KP_Enter].includes(key)){this.inlineText.newlineAtLineEnd();return true;}
            return false;
        }
        if (focus instanceof Gtk.Entry || focus instanceof Gtk.Text || focus instanceof Gtk.TextView) return false;
        if (key === Gdk.KEY_Escape) this.finish(null);
        else if (this.overlay && (key === Gdk.KEY_Shift_L || key === Gdk.KEY_Shift_R)) {
            this.hexColor = !this.hexColor; this._positionMagnifier();
        } else if (this.overlay && !ctrl && (key === Gdk.KEY_c || key === Gdk.KEY_C)) {
            Gdk.Display.get_default().get_clipboard().set(this._pixel().value);
        }
        else if (key === Gdk.KEY_Return || key === Gdk.KEY_KP_Enter || (ctrl && key === Gdk.KEY_c)) this.finish(this.defaultAction);
        else if ([Gdk.KEY_Delete,Gdk.KEY_BackSpace].includes(key)&&this.editor.current) this.editor.remove();
        else if (key===Gdk.KEY_Tab&&this.tool==='arrow') this._changeStyle({arrowEnd:!(this.editor.current??this.editor.style('arrow')).arrowEnd});
        else if (historyShortcut(key, ctrl, shift)) this[historyShortcut(key, ctrl, shift) === 'undo' ? '_undo' : '_redo']();
        else if (key >= Gdk.KEY_1 && key <= Gdk.KEY_9) this._tool(TOOL_SHORTCUTS[key - Gdk.KEY_1]);
        else if (!this.selectingImage && [Gdk.KEY_Page_Up,Gdk.KEY_Page_Down,Gdk.KEY_Tab,Gdk.KEY_ISO_Left_Tab,
            Gdk.KEY_f,Gdk.KEY_F,Gdk.KEY_Left,Gdk.KEY_Right,Gdk.KEY_Up,Gdk.KEY_Down].includes(key)) return false;
        else if (key === Gdk.KEY_Page_Up || key === Gdk.KEY_Page_Down) {
            const next = historyArea(this.historyAreas, this.historyIndex, key === Gdk.KEY_Page_Up ? 1 : -1, this.capture.historyLoop);
            if (next !== null) { this.historyIndex = next; this.rect = intersect(this.historyAreas[next], this.limit); this.selected = true; }
        } else if (key === Gdk.KEY_Tab || key === Gdk.KEY_ISO_Left_Tab) {
            const current = monitorAt(this.monitors, this.rect.x + this.rect.width / 2, this.rect.y + this.rect.height / 2);
            this.rect = {...this.monitors[(this.monitors.indexOf(current) + (shift ? this.monitors.length - 1 : 1)) % this.monitors.length]}; this.selected = true;
        } else if (key === Gdk.KEY_f || key === Gdk.KEY_F) { this.rect = {...this.limit}; this.selected = true; }
        else if ([Gdk.KEY_Left, Gdk.KEY_Right, Gdk.KEY_Up, Gdk.KEY_Down].includes(key)) {
            const dx = key === Gdk.KEY_Left ? -1 : key === Gdk.KEY_Right ? 1 : 0;
            const dy = key === Gdk.KEY_Up ? -1 : key === Gdk.KEY_Down ? 1 : 0;
            const r = this.rect;
            this.rect = shift ? {...r, width: clamp(r.width + dx, 2, this.limit.x + this.limit.width - r.x), height: clamp(r.height + dy, 2, this.limit.y + this.limit.height - r.y)}
                : {...r, x: clamp(r.x + dx, this.limit.x, this.limit.x + this.limit.width - r.width), y: clamp(r.y + dy, this.limit.y, this.limit.y + this.limit.height - r.height)};
            this.selected = true;
        } else {
            const action = {p: 'pin', s: 'quick-save', o: 'ocr_clipboard', l: 'scroll', v: 'record'}[String.fromCharCode(key).toLowerCase()];
            if (!action) return false;
            this.finish(action);
        }
        if (this.resolve) this._update(); return true;
    }
    _queueUpdate() {
        if(this.updateTick || !this.overlay?.views.length)return;
        const widget=this.overlay.views.find(v=>v.drawing.get_mapped())?.drawing;
        if(!widget){this._update();return;}
        this.updateWidget=widget;
        this.updateTick=widget.add_tick_callback(()=>{
            this.updateTick=0;this.updateWidget=null;this._update();return GLib.SOURCE_REMOVE;
        });
    }
    _update() {
        if(!this.editor.textEdit&&this.inlineText?.buffer)this.inlineText.destroy();
        this.inlineText?.flush();
        const r = this.rect;
        this.options?.update();
        for (const [kind, enabled] of [['undo', this.history.canUndo], ['redo', this.history.canRedo]]) {
            const button = this.controls.get(kind);
            button.sensitive = enabled; button.opacity = enabled ? 1 : 0.35;
        }
        const touched = this.monitors.filter(m => { const i = intersect(m, r); return i.width && i.height; }).length;
        const [, , pixelWidth, pixelHeight] = textureRect(r, this.image.getWidth() / this.limit.width, this.limit);
        this.hint.set_label(t("{0} × {1} px", pixelWidth, pixelHeight) +
            (touched > 1 ? ` · ${t("Across {0} monitors", touched)}` : ''));
        if(this.renderedTool!==this.tool){
            for (const [tool, button] of this.buttons) (tool === this.tool||(tool==='rect'&&this.tool==='ellipse')) ? button.add_css_class('suggested-action') : button.remove_css_class('suggested-action');
            for(const b of this.buttons.values())b.get_child()?.queue_draw();
            this.renderedTool=this.tool;
        }
        this._positionUI();
        this.overlay.views.forEach(v => {
            // History may restore marks while Select is active; update picking
            // as well as pixels, without waiting for a pin layout or tool change.
            if(this.embedded)v.drawing.can_target=this.tool!=='select'||this.marks.length>0;
            v.drawing.queue_draw();v.handles?.queue_draw();this._cursor(v.drawing);if(v.surface)this.embedded.pins._inputRegion(v.surface);
        });
    }
    _draw(ctx, width, height, viewport = null) {
        const l = this.limit, r = this.rect;
        this.scale = viewport ? 1 : Math.min(width / l.width, height / l.height);
        this.tx = viewport ? l.x - viewport.x : (width - l.width * this.scale) / 2;
        this.ty = viewport ? l.y - viewport.y : (height - l.height * this.scale) / 2;
        if(this.embedded){ctx.setOperator(Cairo.Operator.CLEAR);ctx.paint();ctx.setOperator(Cairo.Operator.OVER);}
        else {ctx.setSourceRGB(0.10,0.12,0.15);ctx.paint();}
        ctx.translate(this.tx, this.ty); ctx.scale(this.scale, this.scale);
        ctx.save(); ctx.scale(l.width / this.image.getWidth(), l.height / this.image.getHeight());
        ctx.setSourceSurface(this.image, 0, 0); ctx.paint(); ctx.restore();
        ctx.translate(-l.x, -l.y);
        if(this.selectingImage){
            drawCaptureMask(ctx, r, l, this.capture);
            drawCaptureGuides(ctx, this.pointer, l, this.monitors, this.capture);
            drawCaptureFrame(ctx, r, this.capture, {anchors: this.selected, scale: this.scale});
        }
        this._drawMarks(ctx);
    }
    _drawMarks(ctx) {
        const r=this.rect;
        ctx.save();ctx.rectangle(r.x,r.y,r.width,r.height);ctx.clip();
        drawAnnotations(ctx,[...this.editor.displayMarks,...(this.mark?[this.mark]:[])],{
            unit:this.editor.unit,source:this.image,sourceRect:this.limit,cache:this.mosaicCache});
        drawDraftBounds(ctx,this.mark,this.editor.unit,this.scale??1);
        ctx.restore();
        if(!this.embedded)this._drawControls(ctx);
    }
    controlRects(scale=this.scale??1,interactive=false){
        if(interactive&&this.editor.textEdit?.preedit)return [];
        const hover=this.pointer&&!this.mark?this.editor.hover(this.pointer,this.tool,scale):null;
        return [this.editor.controlMark,hover].filter(Boolean).flatMap(mark=>markHandles(mark,this.editor.unit,scale)).map(h=>({x:h.point[0]-8/scale,y:h.point[1]-8/scale,width:16/scale,height:16/scale}));
    }
    _drawControls(ctx){
        drawElementHover(ctx,this.pointer&&!this.mark?this.editor.hover(this.pointer,this.tool,this.scale??1):null,this.editor.unit,this.scale??1,this.accent);
        drawElementSelection(ctx,this.editor.controlMark,this.editor.unit,this.scale??1,this.annotationAnchorColor??'#000000',this.accent,this.pointer);
    }
    open(cancellable) {
        return new Promise((resolve, reject) => {
            this.resolve = resolve; this.reject = reject; this.cancellable = cancellable;
            this.stopAppearance = observeCaptureAppearance(this.appearanceSettings, appearance => {
                Object.assign(this.capture,appearance); this._update();
            });
            this.stopTheme=observeToolbarTheme(this.appearanceSettings,theme=>{
                applyGtkTheme(this.appearanceSettings);
                this.toolbarInk=theme.ink;
                this.toolbarThemeName=theme.name;this.toolbarAppearance=theme;
                this.annotationAnchorColor=theme.anchor;this.accent=theme.selected;this.toolbarSelectedInk=theme.selectedInk;
                this.controlProvider??=new Gtk.CssProvider();this.controlProvider.load_from_string(toolbarControlCss(theme));
                if(!this.controlsStyled){Gtk.StyleContext.add_provider_for_display(Gdk.Display.get_default(),this.controlProvider,Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION+1);this.controlsStyled=true;}
                for(const panel of [this.toolbar,this.options.widget])theme.name==='dark'?panel.add_css_class('clivuu-theme-dark'):panel.remove_css_class('clivuu-theme-dark');
                for(const button of this.controls.values())button.get_child()?.queue_draw();
                this.options.update();
                this.overlay.views.forEach(view=>view.drawing.queue_draw());
            });
            this.cancel = cancellable?.connect(() => this.finish(null));
            if (cancellable?.is_cancelled()) this.finish(null);
            else { this.overlay.watchFocus?.(this.capture.abortOnFocusOut, () => this.finish(null)); this.overlay.open(); }

        });
    }
    finish(action) {
        if (!this.resolve) return;
        if(action)this._commitText?.();else{this.editor?.cancelText();this.inlineText?.destroy();}
        let previewTexture;
        // Reuse the decoded crop for display. The original PNG still goes
        // through the lossless worker, preserving straight-alpha bytes too.
        if ((action === 'pin' || (action && this.previewPin)) && !this.marks.length && this.texture && !this.failure) {
            const [x,y,w,h]=textureRect(this.rect,this.image.getWidth()/this.limit.width,this.limit);
            if(w>=2 && h>=2 && w<=4096 && h<=4096 && w*h<=8_000_000 && x>=0 && y>=0 &&
                x+w<=this.image.getWidth() && y+h<=this.image.getHeight())
                previewTexture=Gdk.Texture.new_for_pixbuf(Gdk.pixbuf_get_from_surface(this.image,x,y,w,h));
        }
        const resolve = this.resolve; this.resolve = null;
        this.stopAppearance?.(); this.stopAppearance = null;
        this.stopTheme?.();this.stopTheme=null;
        this.options?.destroy?.();
        if (this.cancel) {
            // Cancellation callbacks may not disconnect themselves.
            const cancel = this.cancel; this.cancel = 0;
            GLib.idle_add(GLib.PRIORITY_DEFAULT, () => {
                this.cancellable.disconnect(cancel); return GLib.SOURCE_REMOVE;
            });
        }
        if(this.updateTick){this.updateWidget.remove_tick_callback(this.updateTick);this.updateTick=0;this.updateWidget=null;}
        if(this.toolbarTick){this.grip.remove_tick_callback(this.toolbarTick);this.toolbarTick=0;}
        const retained=this.overlay.destroy(action);
        if(this.controlProvider)Gtk.StyleContext.remove_provider_for_display(Gdk.Display.get_default(),this.controlProvider);
        if (this.provider) Gtk.StyleContext.remove_provider_for_display(Gdk.Display.get_default(), this.provider);
        if(retained!==true)this._releaseImage();
        if (this.failure) this.reject(this.failure);
        else resolve(action && (['close','destroy'].includes(action) || (this.rect.width >= 2 && this.rect.height >= 2)) ? {action, rect: this.rect, marks: this.marks, previewTexture} : null);
    }
    _releaseImage(){clearMosaicCache(this.mosaicCache);this.image.finish();this.texture=null;}
}
