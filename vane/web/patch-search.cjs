const fs = require('node:fs');
const path = require('node:path');

// Fixed Vane v1.12.2 bundles; the tracking comment also documents the older entrypoint patches.
// https://github.com/geraldpeng6/wonton-plugin-vane/pull/2#issuecomment-6602296059
function replaceOnce(source, before, after, file) {
  const count = (value) => source.split(value).length - 1;
  if (count(before) === 0 && count(after) === 1) return source;
  if (count(before) !== 1 || count(after) !== 0) {
    throw new Error(`Search error patch does not match the pinned bundle: ${file}`);
  }
  return source.replace(before, after);
}

function patch(file, replacements) {
  const fullPath = path.join('/home/vane/.next/server', file);
  let source = fs.readFileSync(fullPath, 'utf8');
  for (const [before, after] of replacements) {
    source = replaceOnce(source, before, after, file);
  }
  fs.writeFileSync(fullPath, source);
}

// Keep the quality-mode search path intact; only remove the embedding-error fallback.
patch('chunks/641.js', [[
  'catch(a){j=i.results.map(a=>({content:a.content||a.title,metadata:{title:a.title,url:a.url,similarity:1,embedding:[]}}))}finally{g.push(...j)}',
  'catch(error){throw new Error("Embedding failed: "+(error?.message??String(error))+(error?.cause?.code?" ("+error.cause.code+")":""),{cause:error})}finally{g.push(...j)}',
]]);

const errorEvent = 'console.error("Search failed:",error);a.emit("error",{data:error instanceof Error?error.message:String(error)});';

// Both callers start searchAsync without awaiting it; rejection must become a session error.
patch('app/api/chat/route.js', [
  ['async searchAsync(a,b){let c=await i.A.query.messages.findFirst',
    'async searchAsync(a,b){try{let c=await i.A.query.messages.findFirst'],
  ['.execute()}}let n=m;',
    '.execute()}catch(error){' + errorEvent +
    'await i.A.update(j.messages).set({status:"error",responseBlocks:a.getAllBlocks()}).where((0,k.Uo)((0,k.eq)(j.messages.chatId,b.chatId),(0,k.eq)(j.messages.messageId,b.messageId))).execute()}}}let n=m;'],
]);

patch('app/api/search/route.js', [
  ['async searchAsync(a,b){let c=await (0,f.L)',
    'async searchAsync(a,b){try{let c=await (0,f.L)'],
  ['a.emit("end",{})}}let l=k;',
    'a.emit("end",{})}catch(error){' + errorEvent + '}}}let l=k;'],
  ['"error"===e&&b(Response.json({message:"Search error",error:f},{status:500}))',
    '"error"===e&&a(Response.json({message:"Search error",error:f},{status:500}))'],
  ['if("error"===c){if(m.aborted)return;a.error(d)}',
    'if("error"===c){if(m.aborted)return;a.enqueue(k.encode(JSON.stringify({type:"error",data:d.data})+"\\n")),a.close()}'],
]);
