import { useState } from 'react';
import { useLocalLibrary } from './useLocalLibrary';
import type { Video } from '../../lib/types';
import { VideoThumbnail } from './VideoThumbnail';
import { CatSignature } from './CatSignature';
const states: Record<string,string> = { ready:'可改名', waiting:'等待复制完成', needs_review:'需要检查', error:'改名未完成', missing:'文件丢失', ingesting:'正在改名' };
export function LibraryPanel() {
  const { library, issue, busy, choose, scan, page, setPage, applyCount } = useLocalLibrary();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [count, setCount] = useState('');
  if (!library) return <section className="setup-panel" aria-live="polite"><h2>{issue?.message ?? '正在读取文件夹…'}</h2></section>;
  const disabled = busy || library.scanning;
  const selected = library.videos.find(video => video.id === selectedId);
  const validCount = /^\d+$/.test(count) && Number.isSafeInteger(Number(count)) && Number(count) <= 1_000_000;
  const select = (video: Video) => { setSelectedId(video.id); setCount(String(video.useCount)); };
  return <section className="library-panel">
    {!library.rootPath ? <div className="setup-panel"><span className="panel-icon" aria-hidden="true">▣</span><h2>选择视频所在的文件夹</h2><p>文件夹中的视频会自动改成日期时间名称，新放入的视频也会自动处理。</p><button disabled={disabled} onClick={() => { void choose(); }}>{busy ? '正在打开目录…' : '选择素材根目录'}</button></div>
      : <><div className="root-toolbar"><div><span className="eyebrow">当前文件夹 · 打开软件时自动监听</span><p>{library.rootPath}</p></div><div className="toolbar-actions"><button className="secondary" disabled={disabled} onClick={() => { void choose(); }}>选择文件夹</button><button className="secondary" disabled={disabled} onClick={() => { void scan(); }}>{disabled ? '正在处理…' : '重新扫描'}</button></div></div>
        <div className="list-title"><span>{library.total} 个视频 · 按日期时间从新到旧排列</span><span className="listen-status">● {disabled ? '正在处理文件' : '自动监听中'}</span></div>
        <div className="rename-workspace"><div className="file-list" role="listbox" aria-label="选择视频">{library.videos.map(video => <button type="button" role="option" aria-selected={video.id===selectedId} key={video.id} className={`file-row ${video.id===selectedId ? 'selected' : ''}`} disabled={busy} onClick={() => select(video)}><VideoThumbnail video={video} /><span className="file-name">{video.filename}<small>{states[video.status] ?? video.status} · 使用次数 {video.useCount}</small></span></button>)}{library.videos.length===0 && <p className="empty-list">将 MP4、MOV、M4V、AVI 或 MKV 放入文件夹，复制完成后自动改名。</p>}</div>
          <div className="count-editor"><span className="eyebrow">修改使用次数</span><h2>这个视频用了几次？</h2>{selected ? <><p className="selected-filename">{selected.filename}</p><form onSubmit={event => { event.preventDefault(); if (validCount && !disabled && selected.status==='ready') void applyCount(selected,Number(count)); }}><label htmlFor="usage-count">该视频用了</label><div className="count-field"><input id="usage-count" type="number" min="0" max="1000000" step="1" value={count} onChange={event => setCount(event.target.value)} disabled={disabled || selected.status!=='ready'} /><span>次</span></div><button className="apply-button" type="submit" disabled={disabled || !validCount || selected.status!=='ready'}>{busy ? '正在改名…' : '保存次数并改名'}</button></form><p className="editor-hint">日期时间保持不变，只修改使用次数。</p></> : <p>先从左边选择一个视频，再填写次数。</p>}<CatSignature /></div></div>
        {library.total>100 && <div className="pagination"><button className="secondary" disabled={disabled || page===0} onClick={()=>{setSelectedId(null);setPage(page-1);}}>上一页</button><span>第 {page+1} / {Math.ceil(library.total/100)} 页</span><button className="secondary" disabled={disabled || (page+1)*100>=library.total} onClick={()=>{setSelectedId(null);setPage(page+1);}}>下一页</button></div>}
        {library.candidates.length>0 && <div className="pending-files"><p>等待处理</p>{library.candidates.map(candidate=><p key={candidate.id}>{candidate.filename} · {states[candidate.status]}</p>)}</div>}
      </>}
    {(issue ?? library.issue) && <div className="notice" role="alert">{(issue ?? library.issue)?.message}</div>}
  </section>;
}

