// Rust literals and their offsets, excluding nested comments and characters.
export function rustSource(source) {
 const strings=[],masked=source.split('');let i=0;
 const hide=(a,b)=>{for(let n=a;n<b;n++)if(masked[n]!=='\n')masked[n]=' ';};
 while(i<source.length){
  const start=i;
  if(source.startsWith('//',i)){const end=source.indexOf('\n',i);i=end<0?source.length:end;hide(start,i);continue;}
  if(source.startsWith('/*',i)){i+=2;let depth=1;while(i<source.length&&depth){if(source.startsWith('/*',i)){depth++;i+=2;}else if(source.startsWith('*/',i)){depth--;i+=2;}else i++;}hide(start,i);continue;}
  const char=source.slice(i).match(/^'(?:[^'\\\n]|\\(?:u\{[\da-fA-F]+\}|.))'/);
  if(char){i+=char[0].length;hide(start,i);continue;}
  const raw=source.slice(i).match(/^(?:br|cr|r)(#*)"/);
  if(raw){const begin=i+raw[0].length,end=source.indexOf('"'+raw[1],begin);if(end<0)throw new Error('Unterminated Rust raw string');i=end+1+raw[1].length;strings.push({text:source.slice(begin,end),start,end:i});hide(start,i);continue;}
  if(source[i]==='"'){
   i++;while(i<source.length){if(source[i]==='\\')i+=2;else if(source[i++]==='"')break;}
   const quoted=source.slice(start,i).replace(/\\\r?\n\s*/g,'').replace(/\\u\{([\da-fA-F]+)\}/g,(_,n)=>JSON.stringify(String.fromCodePoint(parseInt(n,16))).slice(1,-1)).replace(/\\x([\da-fA-F]{2})/g,'\\u00$1').replace(/\\0/g,'\\u0000');
   strings.push({text:JSON.parse(quoted),start,end:i});hide(start,i);continue;
  }
  i++;
 }
 return {strings,masked:masked.join('')};
}
export function translatedLiterals(source) {
 const {strings,masked}=rustSource(source),messages=[];
 for(const match of masked.matchAll(/\b(?:t|tf|translate_system)\s*\(/g)){
  const start=match.index+match[0].length;let end=start,depth=0;
  while(end<masked.length){const c=masked[end];if('([{'.includes(c))depth++;else if(')]}'.includes(c)){if(depth===0)break;depth--;}else if(c===','&&depth===0)break;end++;}
  messages.push(...strings.filter(s=>s.start>=start&&s.end<=end));
 }
 return messages;
}
// Direct GTK text needs translation even when the catalog does not yet contain it.
export function literalTextSinks(source) {
 return rustSource(source).strings.filter(({start})=>/(?:\.(?:label|title|subtitle|description|tooltip_text|placeholder_text|set_label|set_title|set_subtitle|set_tooltip_text)|::with_label|Label::new)\s*\(\s*(?:Some\(\s*)?&?\s*$/.test(source.slice(Math.max(0,start-100),start)));
}
