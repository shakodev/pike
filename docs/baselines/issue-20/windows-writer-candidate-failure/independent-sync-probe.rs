
use alacritty_terminal::{event::{Event,EventListener,OnResize,WindowSize}, event_loop::EventLoop, grid::Dimensions, sync::FairMutex, term::Term, tty::{ChildEvent,EventedPty,EventedReadWrite}};
use std::{io::{self,Read,Write},sync::{Arc,atomic::{AtomicUsize,Ordering},mpsc},net::{TcpListener,TcpStream,Ipv4Addr},time::Duration};
#[derive(Clone)] struct Listener;
impl EventListener for Listener {fn send_event(&self,_:Event){}}
struct Size; impl Dimensions for Size {fn total_lines(&self)->usize{24} fn screen_lines(&self)->usize{24} fn columns(&self)->usize{80}}
struct Blocked {calls:Arc<AtomicUsize>}
impl Write for Blocked {fn write(&mut self,bytes:&[u8])->io::Result<usize>{let n=self.calls.fetch_add(1,Ordering::SeqCst);if n==0 {Ok(1.min(bytes.len()))}else{Err(io::ErrorKind::WouldBlock.into())}} fn flush(&mut self)->io::Result<()>{Ok(())}}
struct Pty {reader:TcpStream,writer:Blocked}
impl OnResize for Pty{fn on_resize(&mut self,_:WindowSize){}}
impl EventedReadWrite for Pty {
 type Reader=TcpStream;type Writer=Blocked;
 unsafe fn register(&mut self,p:&Arc<polling::Poller>,_:polling::Event,m:polling::PollMode)->io::Result<()>{unsafe{p.add_with_mode(&self.reader,polling::Event::readable(0),m)}}
 fn reregister(&mut self,p:&Arc<polling::Poller>,_:polling::Event,m:polling::PollMode)->io::Result<()>{p.modify_with_mode(&self.reader,polling::Event::readable(0),m)}
 fn deregister(&mut self,p:&Arc<polling::Poller>)->io::Result<()>{p.delete(&self.reader)}
 fn reader(&mut self)->&mut TcpStream{&mut self.reader} fn writer(&mut self)->&mut Blocked{&mut self.writer}
}
impl EventedPty for Pty{fn next_child_event(&mut self)->Option<ChildEvent>{None}}
fn main(){
 let listener=TcpListener::bind((Ipv4Addr::LOCALHOST,0)).unwrap();let mut peer=TcpStream::connect(listener.local_addr().unwrap()).unwrap();let (reader,_)=listener.accept().unwrap();reader.set_nonblocking(true).unwrap();
 let calls=Arc::new(AtomicUsize::new(0));let terminal=Arc::new(FairMutex::new(Term::new(Default::default(),&Size,Listener)));
 let event_loop=EventLoop::new(terminal.clone(),Listener,Pty{reader,writer:Blocked{calls:calls.clone()}},true,false).unwrap();let sender=event_loop.channel();
 peer.write_all(b"\x1b[?2026hSYNC_SENTINEL").unwrap();sender.send_input(b"abc").unwrap();let(done,completion)=mpsc::channel();event_loop.spawn_disposing(||{},move|c|done.send(c).unwrap());
 std::thread::sleep(Duration::from_millis(300));let write_calls=calls.load(Ordering::SeqCst);let status=sender.input_status();let text:String=terminal.lock().grid().display_iter().map(|c|c.c).collect();let flushed=text.contains("SYNC_SENTINEL");
 println!("write_calls_after_300ms={write_calls}; queued={}; outstanding={}; sync_sentinel_flushed={flushed}",status.queued_bytes,status.outstanding_bytes);
 sender.send(alacritty_terminal::event_loop::Msg::Shutdown).unwrap();let result=completion.recv_timeout(Duration::from_secs(5)).unwrap();println!("cleanup={:?}",result.reason);
 assert!(write_calls<100,"busy loop: expired sync deadline retried blocked writer {write_calls} times");assert!(flushed,"expired synchronized output remained buffered");
}
