// Own vector artwork; no icon-font dependency or bitmap scaling on the toolbar.
export function drawToolIcon(ctx, width, height, kind, ink = '#e8edf5', filled = false) {
    ctx.scale(width / 24, height / 24);
    ctx.setSourceRGB(...[1,3,5].map(i=>parseInt(ink.slice(i,i+2),16)/255)); ctx.setLineWidth(1.6); ctx.setLineCap(1); ctx.setLineJoin(1);
    const line = (...p) => { ctx.moveTo(p[0], p[1]); for (let i = 2; i < p.length; i += 2) ctx.lineTo(p[i], p[i + 1]); };
    switch (kind) {
    case 'brush':ctx.arc(12,12,3.5,0,Math.PI*2);ctx.fill();return;
    case 'fill-pattern':
        for(let y=4;y<20;y+=2)for(let x=4;x<20;x+=2)if(((x+y)/2)%2===0)ctx.rectangle(x,y,2,2);
        ctx.fill();return;
    case 'replace': line(4, 12, 10, 18, 21, 5); break;
    case 'crop': line(3, 6, 18, 6, 18, 21); line(6, 3, 6, 18, 21, 18); break;
    case 'rotate-right': ctx.arc(12, 12, 7, -2.7, 2.7); line(7, 3, 7, 9, 2, 7); break;
    case 'rotate-left': ctx.arc(12, 12, 7, -0.4, 4.9); line(17, 3, 17, 9, 22, 7); break;
    case 'flip-horizontal': line(10, 3, 10, 21); line(7, 6, 3, 18, 7, 18, 7, 6); line(14, 6, 21, 18, 14, 18, 14, 6); break;
    case 'flip-vertical': line(3, 11, 21, 11); line(6, 7, 18, 7, 12, 3, 6, 7); line(6, 15, 18, 15, 12, 21, 6, 15); break;
    case 'select': line(5, 3, 5, 19, 9, 15, 12, 21, 15, 19, 12, 13, 18, 13, 5, 3); break;
    case 'rect': ctx.rectangle(4, 6, 16, 12); break;
    case 'shape':ctx.rectangle(3,4,12,10);ctx.stroke();ctx.save();ctx.translate(15.5,15.5);ctx.scale(6,4.5);ctx.arc(0,0,1,0,Math.PI*2);ctx.restore();break;
    case 'ellipse': ctx.save(); ctx.translate(12, 12); ctx.scale(8, 6); ctx.arc(0, 0, 1, 0, Math.PI * 2); ctx.restore(); break;
    case 'arrow': line(5, 19, 19, 5, 11, 5); line(19, 5, 19, 13); break;
    case 'pen': line(4, 20, 6, 13, 16, 3, 21, 8, 11, 18, 4, 20); line(6, 13, 11, 18); break;
    case 'highlight': line(4, 17, 8, 12, 12, 16, 7, 20); line(8, 12, 16, 4, 20, 8, 12, 16); line(3, 21, 13, 21); break;
    case 'highlight-free':
        ctx.setLineWidth(4);ctx.moveTo(4,16);ctx.curveTo(6,5,10,5,12,12);ctx.curveTo(14,19,18,19,20,8);ctx.stroke();return;
    case 'highlight-line':
        ctx.setLineWidth(6);ctx.setLineCap(0);line(4,15,20,9);ctx.stroke();return;
    case 'mosaic':
        // Four touching tiles form one recognisable pixelated patch at 18px.
        ctx.rectangle(4,4,16,16);ctx.stroke();
        ctx.rectangle(4,4,8,8);ctx.rectangle(12,12,8,8);ctx.fill();
        line(12,4,12,20);line(4,12,20,12);break;
    case 'text': line(5, 5, 19, 5); line(12, 5, 12, 20); line(8, 20, 16, 20); break;
    case 'text-bold':
        ctx.setLineWidth(2.7);line(6,4,6,20);ctx.moveTo(6,4);ctx.lineTo(12,4);ctx.curveTo(20,4,20,11,12,11);ctx.lineTo(6,11);ctx.moveTo(12,11);ctx.curveTo(22,11,22,20,12,20);ctx.lineTo(6,20);break;
    case 'text-italic':
        ctx.setLineWidth(2);line(10,4,20,4);line(15,4,9,20);line(4,20,14,20);break;
    case 'text-outline':
        ctx.setLineWidth(1.2);line(3,20,9.5,4,14.5,4,21,20,16,20,14.5,16,9.5,16,8,20,3,20);line(11,12,12,8.5,13,12,11,12);break;
    case 'eraser': line(3,14,13,4,21,12,12,21,10,21,3,14);line(8,9,16,17);line(10,21,21,21);break;
    case 'undo': line(9, 5, 4, 10, 9, 15); ctx.moveTo(4, 10); ctx.curveTo(22, 4, 24, 23, 10, 20); break;
    case 'redo': line(15, 5, 20, 10, 15, 15); ctx.moveTo(20, 10); ctx.curveTo(2, 4, 0, 23, 14, 20); break;
    case 'clipboard': ctx.rectangle(4, 7, 12, 14); line(9, 7, 9, 3, 21, 3, 21, 17, 16, 17); break;
    case 'pin': line(7,3,17,3,15.5,10,19,14,5,14,8.5,10,7,3);line(12,14,12,21);break;
    case 'quick-save': line(4, 3, 18, 3, 21, 6, 21, 21, 3, 21, 3, 3); ctx.rectangle(7, 3, 9, 6); ctx.rectangle(7, 14, 10, 7); break;
    case 'ocr_clipboard': line(3, 8, 3, 3, 8, 3); line(16, 3, 21, 3, 21, 8); line(3, 16, 3, 21, 8, 21); line(16, 21, 21, 21, 21, 16); line(7, 16, 12, 7, 17, 16); line(9, 13, 15, 13); break;
    case 'scroll': ctx.rectangle(6, 3, 12, 18); line(9, 12, 12, 16, 15, 12); line(12, 7, 12, 16); break;
    case 'record': ctx.rectangle(3, 6, 12, 12); line(15, 10, 21, 6, 21, 18, 15, 14); break;
    case 'grip': for (const x of [9, 15]) for (const y of [6, 12, 18]) { ctx.rectangle(x, y, 1, 1); } break;
    default: line(6, 6, 18, 18); line(6, 18, 18, 6);
    }
    if(filled)ctx.fill();else ctx.stroke();
}
