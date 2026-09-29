import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { LibraryPanel } from './LibraryPanel';
import { chooseLibrary, getLibraryStatus, rescanLibrary, setUsage } from '../../lib/desktop';
import type { LibraryStatus } from '../../lib/types';
vi.mock('../../lib/desktop', () => ({ getLibraryStatus:vi.fn(),chooseLibrary:vi.fn(),rescanLibrary:vi.fn(),reportFrontendError:vi.fn(),setUsage:vi.fn() }));
const empty: LibraryStatus = {id:null,rootPath:null,scanning:false,total:0,ready:0,issue:null,candidates:[],videos:[],page:0};
const selected: LibraryStatus = {...empty,id:'library',rootPath:'/中文素材 (1)',total:1,ready:1,videos:[{id:'a',filename:'2026-09-28_14-35-08.mp4',currentPath:'/中文素材 (1)/2026-09-28_14-35-08.mp4',useCount:0,status:'ready'}]};
describe('简单改名工具', () => {
  beforeEach(() => vi.mocked(getLibraryStatus).mockResolvedValue(empty));
  it('快速点击只打开一次目录选择器', async () => {
    let resolve!: (value: LibraryStatus|null) => void;
    vi.mocked(chooseLibrary).mockReturnValue(new Promise(value => {resolve=value;}));
    render(<LibraryPanel/>);
    const button = await screen.findByRole('button',{name:'选择素材根目录'});
    fireEvent.click(button); fireEvent.click(button);
    expect(chooseLibrary).toHaveBeenCalledOnce(); expect(button).toBeDisabled();
    resolve(selected); expect(await screen.findByRole('option')).toHaveTextContent('2026-09-28_14-35-08.mp4');
  });
  it('取消选择不会产生错误', async () => {
    vi.mocked(chooseLibrary).mockResolvedValue(null); render(<LibraryPanel/>);
    fireEvent.click(await screen.findByRole('button',{name:'选择素材根目录'}));
    await waitFor(() => expect(screen.getByRole('button',{name:'选择素材根目录'})).toBeEnabled());
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });
  it('扫描错误保留当前列表', async () => {
    vi.mocked(getLibraryStatus).mockResolvedValue(selected);
    vi.mocked(rescanLibrary).mockRejectedValue({code:'FILESYSTEM',message:'请检查素材目录是否已连接。',retryable:true});
    render(<LibraryPanel/>); fireEvent.click(await screen.findByRole('button',{name:'重新扫描'}));
    expect(await screen.findByRole('alert')).toHaveTextContent('请检查素材目录是否已连接。');
    expect(screen.getByRole('option')).toBeInTheDocument();
  });
  it('提交填写的绝对次数并立即禁用，快速点击不会重复发送', async () => {
    vi.mocked(getLibraryStatus).mockResolvedValue(selected);
    let resolve!: () => void;
    vi.mocked(setUsage).mockReturnValue(new Promise<void>(value=>{resolve=value;}));
    render(<LibraryPanel/>); fireEvent.click(await screen.findByRole('option'));
    fireEvent.change(screen.getByLabelText('该视频用了'),{target:{value:'5'}});
    const button=screen.getByRole('button',{name:'保存次数并改名'});
    fireEvent.click(button);fireEvent.click(button);
    expect(button).toBeDisabled();expect(setUsage).toHaveBeenCalledOnce();
    expect(setUsage).toHaveBeenCalledWith('a',0,5,expect.any(String));resolve();
    await waitFor(()=>expect(screen.getByRole('button',{name:'保存次数并改名'})).toBeEnabled());
  });
  it('负数与小数不能提交',async()=>{
    vi.mocked(getLibraryStatus).mockResolvedValue(selected);render(<LibraryPanel/>);fireEvent.click(await screen.findByRole('option'));
    for(const value of ['-1','1.5','1000001']){fireEvent.change(screen.getByLabelText('该视频用了'),{target:{value}});expect(screen.getByRole('button',{name:'保存次数并改名'})).toBeDisabled();}
    expect(setUsage).not.toHaveBeenCalled();
  });
});

