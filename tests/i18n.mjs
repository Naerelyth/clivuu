import assert from 'node:assert/strict';
import {readFileSync,readdirSync} from 'node:fs';
import {join} from 'node:path';
import {rustSource,translatedLiterals,literalTextSinks} from './common/rust-strings.mjs';
const root=new URL('../',import.meta.url).pathname;
const read=path=>readFileSync(join(root,path),'utf8');
const manifest=JSON.parse(read('core/locales/languages.json'));
const invariants=JSON.parse(read('tests/i18n-invariants.json'));
const catalogs=new Map(),tags=new Map();
for(const entry of manifest.languages){
 assert.ok(!catalogs.has(entry.id),`duplicate language ${entry.id}`);
 assert.ok(entry.name.length&&entry.aliases.every(a=>a.length));
 for(const tag of [entry.id,...entry.aliases]){
  const normalized=tag.toLowerCase().replaceAll('_','-');
  assert.ok(!tags.has(normalized)||tags.get(normalized)===entry.id,`ambiguous locale alias ${tag}`);tags.set(normalized,entry.id);
 }
 const raw=read(`core/locales/${entry.catalog}`),catalog=JSON.parse(raw);
 const tokens=[...raw.matchAll(/"(?:\\.|[^"\\])*"|[{}:,]/g)].map(m=>m[0]);
 const keys=tokens.filter((token,index)=>token.startsWith('"')&&tokens[index+1]===':').map(token=>JSON.parse(token));
 assert.equal(new Set(keys).size,keys.length,`${entry.id}: duplicate message keys`);
 assert.ok(Object.values(catalog).every(v=>typeof v==='string'&&v.trim().length),`${entry.id}: empty/non-string translation`);
 if(entry.help)assert.ok(read(`core/locales/${entry.help}`).trim().length);
 catalogs.set(entry.id,catalog);
}
const en=catalogs.get(manifest.default);assert.ok(en,'default catalog');
for(const entry of manifest.languages){
 assert.match(entry.id,/^[a-zA-Z]{2,8}(?:-[a-zA-Z0-9]{2,8})*$/);
 for(const [key,reason] of Object.entries(entry.unchanged??{})){
  assert.ok(Object.hasOwn(en,key)&&typeof reason==='string'&&reason.trim().length,`${entry.id}: invalid unchanged exception: ${key}`);
  assert.equal(catalogs.get(entry.id)[key],en[key],`${entry.id}: obsolete unchanged exception: ${key}`);
 }
}
for(const [id,catalog] of catalogs){
 assert.deepEqual(Object.keys(catalog).sort(),Object.keys(en).sort(),`${id}: catalog parity`);
 for(const [key,value] of Object.entries(catalog)){
  assert.deepEqual((en[key].match(/\{\d+\}/g)??[]).sort(),(value.match(/\{\d+\}/g)??[]).sort(),`${id}: placeholders: ${key}`);
  if(id!==manifest.default&&value===en[key])assert.ok(invariants.unchanged.includes(key)||manifest.languages.find(l=>l.id===id).unchanged?.[key],`${id}: untranslated: ${key}`);
 }
}
const invariantText=text=>invariants.unchanged.includes(text)||Object.hasOwn(invariants.literals,text);
const untranslatedGtk=source=>literalTextSinks(source).filter(({text})=>text&&!invariantText(text));
assert.deepEqual(untranslatedGtk('Button::with_label("Save"); let b=Label::new(Some(&t("Cancel"))); Label::new(Some("A newly added untranslated label"));').map(s=>s.text),['Save','A newly added untranslated label']);
// Regression cases that the former immediate-literal regular expression missed.
assert.deepEqual(translatedLiterals('// t("comment")\nlet a=t(if ok {"Yes"} else {r#"No"#}); let b=tf("{0}",&["user data"]);').map(s=>s.text),['Yes','No','{0}']);
assert.deepEqual(rustSource('/* nested /* "comment" */ */ let r=r##"raw "quotes""##;').strings.map(s=>s.text),['raw "quotes"']);
const files=[];
function scan(directory){for(const item of readdirSync(join(root,directory),{withFileTypes:true})){const path=join(directory,item.name);if(item.isDirectory())scan(path);else if(item.isFile()&&path.endsWith('.rs')&&!path.includes('/tests'))files.push(path);}}
for(const path of ['desktop/src','desktop/x11/src','core/src','cli/src'])scan(path);
let count=0;
for(const file of files){
 const source=read(file),check=({text,start})=>{
  if(text==='')return;
  assert.ok(Object.hasOwn(en,text),`${file}:${source.slice(0,start).split('\n').length}: missing translation: ${text}`);count++;
 };
 translatedLiterals(source).forEach(check);
 for(const literal of untranslatedGtk(source)) {
  assert.fail(`${file}:${source.slice(0,literal.start).split('\n').length}: untranslated GTK text: ${literal.text}`);
 }
 if(file.startsWith('desktop/src/ui/')||['desktop/src/editor/shortcuts.rs','core/src/pins/mouse.rs','core/src/annotation/arrows.rs'].includes(file)){
  for(const literal of rustSource(source).strings){
   const {text,start}=literal;
   if(!/[A-Za-z]{3}/.test(text)||(!text.includes(' ')&&!/^[A-Z][a-z]+$/.test(text)))continue;
   if(/\b(?:reason|must_use|doc)\s*=\s*$|\.expect\(\s*$/.test(source.slice(Math.max(0,start-100),start)))continue;
   if(text.includes('font-')||text.includes('background:')||text.includes('{family'))continue;
   if(Object.hasOwn(invariants.literals,text))continue;
   check(literal);
  }
 }
}
console.log(`i18n: ${catalogs.size} registered locales, ${Object.keys(en).length} messages, ${count} source literals, catalog/placeholder/translation integrity passed`);
