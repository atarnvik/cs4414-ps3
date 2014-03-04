//
// zhtta.rs
//
// Starting code for PS3
// Running on Rust 0.9
//
// Note that this code has serious security risks!  You should not run it 
// on any system with access to sensitive files.
// 
// University of Virginia - cs4414 Spring 2014
// Weilin Xu and David Evans
// Version 0.5

// To see debug! outputs set the RUST_LOG environment variable, e.g.: export RUST_LOG="zhtta=debug"

// Problem 7 design decision: cache should not be bigger than 10 MB
    // remove last modified files
    // check cache for files first!

#[feature(globs)];
extern mod extra;

use std::io::*;
use std::io::net::ip::{SocketAddr};
use std::{os, str, libc, from_str};
use std::path::Path;
use std::hashmap::HashMap;


use extra::getopts;
use extra::arc::MutexArc;
use extra::arc::RWArc;
use extra::lru_cache::LruCache;
use extra::priority_queue::PriorityQueue;

use std::io::buffered::BufferedStream;
use std::io::File;
use std::io::fs;


mod gash;

static SERVER_NAME : &'static str = "Zhtta Version 0.5";

static IP : &'static str = "127.0.0.1";
static PORT : uint = 4414;
static WWW_DIR : &'static str = "./www";

static HTTP_OK : &'static str = "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=UTF-8\r\n\r\n";
static HTTP_BAD : &'static str = "HTTP/1.1 404 Not Found\r\n\r\n";

static COUNTER_STYLE : &'static str = "<doctype !html><html><head><title>Hello, Rust!</title>
             <style>body { background-color: #884414; color: #FFEEAA}
                    h1 { font-size:2cm; text-align: center; color: black; text-shadow: 0 0 4mm red }
                    h2 { font-size:2cm; text-align: center; color: black; text-shadow: 0 0 4mm green }
             </style></head>
             <body>";


struct HTTP_Request {
    // Use peer_name as the key to access TcpStream in hashmap. 

    // (Due to a bug in extra::arc in Rust 0.9, it is very inconvenient to use TcpStream without the "Freeze" bound.
    //  See issue: https://github.com/mozilla/rust/issues/12139)
    peer_name: ~str,
    path: ~Path,
}

impl std::cmp::Eq for HTTP_Request {
    fn eq(&self, other: &HTTP_Request) -> bool {
        let sizeSelf = fs::stat(self.path).size;
        let sizeOther = fs::stat(other.path).size;

        if sizeOther == sizeSelf {
            if (other.peer_name.slice_to(7) == "128.143." || other.peer_name.slice_to(6) == "137.54." || other.peer_name.slice_to(9) == "127.0.0.1") && (self.peer_name.slice_to(7) == "128.143." || self.peer_name.slice_to(6) == "137.54." || self.peer_name.slice_to(9) == "127.0.0.1") {
                return true;
            }
        }
        return false;
    }
}

impl std::cmp::Ord for HTTP_Request {
    fn lt(&self, other: &HTTP_Request) -> bool {
        //First get the file sizes for the Http_Request

        let sizeSelf = fs::stat(self.path).size;
        let sizeOther = fs::stat(other.path).size;

        if sizeOther > sizeSelf {
            return true;
        }
        else {
            return getPriority(self.peer_name.clone()) < getPriority(other.peer_name.clone());
        }
    }
}



struct WebServer {
    ip: ~str,
    port: uint,
    www_dir_path: ~Path,
    
    request_queue_arc: MutexArc<PriorityQueue<HTTP_Request>>,
    stream_map_arc: MutexArc<HashMap<~str, Option<std::io::net::tcp::TcpStream>>>,
    cache: MutexArc<LruCache<Path,~[u8]>>,
    
    notify_port: Port<()>,
    shared_notify_chan: SharedChan<()>
}

impl WebServer {
    fn new(ip: &str, port: uint, www_dir: &str) -> WebServer {
        let (notify_port, shared_notify_chan) = SharedChan::new();
        let www_dir_path = ~Path::new(www_dir);
        os::change_dir(www_dir_path.clone());

        WebServer {
            ip: ip.to_owned(),
            port: port,
            www_dir_path: www_dir_path,
                        
            request_queue_arc: MutexArc::new(PriorityQueue::new()),
            stream_map_arc: MutexArc::new(HashMap::new()),
            cache: MutexArc::new(LruCache::new(10)),

            notify_port: notify_port,
            shared_notify_chan: shared_notify_chan        
        }
    }
    
    fn run(&mut self) {
        self.listen();
        self.dequeue_static_file_request();
    }
    
    fn listen(&mut self) {
        let addr = from_str::<SocketAddr>(format!("{:s}:{:u}", self.ip, self.port)).expect("Address error.");
        let www_dir_path_str = self.www_dir_path.as_str().expect("invalid www path?").to_owned();
        
        let request_queue_arc = self.request_queue_arc.clone();
        let shared_notify_chan = self.shared_notify_chan.clone();
        let stream_map_arc = self.stream_map_arc.clone();
                
        spawn(proc() {
            let mut acceptor = net::tcp::TcpListener::bind(addr).listen();
            println!("{:s} listening on {:s} (serving from: {:s}).", 
                     SERVER_NAME, addr.to_str(), www_dir_path_str);

            //Visitor counter
            let num_visitor : uint = 0;
            //Arc for visitor counter.
            let visitor_arc_mut = RWArc::new(num_visitor);            
            
            for stream in acceptor.incoming() {
                let (queue_port, queue_chan) = Chan::new();
                queue_chan.send(request_queue_arc.clone());
                
                let notify_chan = shared_notify_chan.clone();
                let stream_map_arc = stream_map_arc.clone();

                let(portMut, chanMut) = Chan::new();
                chanMut.send(visitor_arc_mut.clone());
                
                // Spawn a task to handle the connection.
                spawn(proc() {
                    let request_queue_arc = queue_port.recv();

                    //This updates counter by adding one to it.
                    let local_arc_mut = portMut.recv();
                    local_arc_mut.write(|value| {
                        *value += 1
                    }); 
                    //This sets a local variable to current count.
                    let mut visitor_count_local : uint = 0;
                    local_arc_mut.read(|value| {
                        //println(value.to_str());
                        visitor_count_local = *value;
                    });
                  
                    let mut stream = stream;
                    
                    let peer_name = WebServer::get_peer_name(&mut stream);
                    
                    let mut buf = [0, ..500];
                    stream.read(buf);
                    let request_str = str::from_utf8(buf);
                    debug!("Request:\n{:s}", request_str);
                    
                    let req_group : ~[&str]= request_str.splitn(' ', 3).collect();
                    if req_group.len() > 2 {
                        let path_str = "." + req_group[1].to_owned();
                        
                        let mut path_obj = ~os::getcwd();
                        path_obj.push(path_str.clone());
                        
                        let ext_str = match path_obj.extension_str() {
                            Some(e) => e,
                            None => "",
                        };
                        
                        debug!("Requested path: [{:s}]", path_obj.as_str().expect("error"));
                        debug!("Requested path: [{:s}]", path_str);
                             
                        if path_str == ~"./" {
                            debug!("===== Counter Page request =====");
                            WebServer::respond_with_counter_page(stream, &visitor_count_local);
                            debug!("=====Terminated connection from [{:s}].=====", peer_name);
                        } else if !path_obj.exists() || path_obj.is_dir() {
                            debug!("===== Error page request =====");
                            WebServer::respond_with_error_page(stream, path_obj);
                            debug!("=====Terminated connection from [{:s}].=====", peer_name);
                        } else if ext_str == "shtml" { // Dynamic web pages.
                            debug!("===== Dynamic Page request =====");
                            WebServer::respond_with_dynamic_page(stream, path_obj);
                            debug!("=====Terminated connection from [{:s}].=====", peer_name);
                        } else { 
                            debug!("===== Static Page request =====");
                            WebServer::enqueue_static_file_request(stream, path_obj, stream_map_arc, request_queue_arc, notify_chan);
                        }
                    }
                });
            }
        });
    }

    fn respond_with_error_page(stream: Option<std::io::net::tcp::TcpStream>, path: &Path) {
        let mut stream = stream;
        let msg: ~str = format!("Cannot open: {:s}", path.as_str().expect("invalid path").to_owned());

        stream.write(HTTP_BAD.as_bytes());
        stream.write(msg.as_bytes());
    }

    // TODO: Safe visitor counter.
    fn respond_with_counter_page(stream: Option<std::io::net::tcp::TcpStream>, visitor_count_local: &uint) {
        let mut stream = stream;
        let visitor_count_other : uint = visitor_count_local.clone();
        let response: ~str = 
            format!("{:s}{:s}<h1>Greetings, Krusty!</h1>
                     <h2>Visitor count: {:u}</h2></body></html>\r\n", 
                    HTTP_OK, COUNTER_STYLE, 
                    visitor_count_other);
        debug!("Responding to counter request");
        stream.write(response.as_bytes());
    }
    
    // TODO: Streaming file.
    // TODO: Application-layer file caching.
    fn respond_with_static_file(stream: Option<std::io::net::tcp::TcpStream>, path: &Path, cache: MutexArc<LruCache<Path, ~[u8]>>) {
        let mut stream = stream;
        stream.write(HTTP_OK.as_bytes());
        cache.access(|local_cache| {
            debug!("Got cache queue mutex lock.");
            let mut bytes = local_cache.get(path);
            match(bytes) {
                Some(bytes) => { 
                                // in cache
                                for &i in bytes.iter() {
                                    stream.write_u8(i);
                                }
                            }
                None =>  {}
            }
        });
        cache.access(|local_cache| {
            // not in cache
            //let mut stream = stream;
            let mut file_reader = File::open(path).expect("Invalid file!");
            let mut byteArray : ~[u8] = ~[];
            let byteLength : uint = 1;
            while(true) {
                match (file_reader.read_byte()) {
                    Some (byte) => {
                                    stream.write_u8(byte);
                                    byteArray.push(byte); 
                                }
                    None => {break}
                }
            }
            // add to cache!
            // automatically handles removing other elements if necessary
            if(fs::stat(path).size < 1000000) {
                local_cache.put(path.clone(), byteArray);
            }
        });
        
    }
    
    // TODO: Server-side gashing.
    fn respond_with_dynamic_page(stream: Option<std::io::net::tcp::TcpStream>, path: &Path) {
        //for now, just serve as static file
        let mut shtml_file = File::open(path);
        let mut rwStream = BufferedStream::new(shtml_file);
        let mut newFile : ~[~str] = ~[];

        let mut checkIfLastIsCmd : bool = false;
        for line in rwStream.lines() {
            let mut check : bool = false;
            let mut newLine : ~[~str] = ~[];
            for split in line.split(' ') {
                if(check) {
                    let cmdSplit : ~[&str] = split.split('=').collect();
                    let command : ~str = cmdSplit[1].to_owned();
                    let finalCommand = command.slice(1,command.len()-1).to_owned();
                    let output : ~str = gash::run_cmdline(finalCommand);
                    newLine.push(output);
                    check = false;
                    checkIfLastIsCmd = true;
                }
                else if(split == "<!--#exec") {
                    check = true;
                }
                else if(split == "-->") {

                }
                else {
                    if(checkIfLastIsCmd && split.slice(0, 3) == "-->") {
                        newLine.push(split.slice_from(3).to_owned());
                        newLine.push(" ".to_owned());
                        checkIfLastIsCmd = false;
                    }
                    else if(split.len() > 9 && split.slice_from(split.len() - 9) == "<!--#exec") {
                        newLine.push(split.slice(0, split.len()-9).to_owned());
                        check = true;                    }
                    else {
                        newLine.push(split.to_owned());
                        newLine.push(" ".to_owned()); 
                    }
                }
            }
            let mut fullLine : ~str = ~"";
            for s in newLine.iter() {
                fullLine = fullLine + s.clone();
            }
            newFile.push(fullLine);
        }
        let mut fullPage : ~str = ~"";
        for s in newFile.iter() {
            fullPage = fullPage + s.clone();
        }
        let mut stream = stream;
        stream.write(HTTP_OK.as_bytes());
        stream.write(fullPage.as_bytes());        
        
    }
    
    // TODO: Smarter Scheduling.
    fn enqueue_static_file_request(stream: Option<std::io::net::tcp::TcpStream>, path_obj: &Path, stream_map_arc: MutexArc<HashMap<~str, Option<std::io::net::tcp::TcpStream>>>, req_queue_arc: MutexArc<PriorityQueue<HTTP_Request>>, notify_chan: SharedChan<()>) {
        // Save stream in hashmap for later response.
        let mut stream = stream;
        let peer_name = WebServer::get_peer_name(&mut stream);
        let (stream_port, stream_chan) = Chan::new();
        stream_chan.send(stream);
        unsafe {
            // Use an unsafe method, because TcpStream in Rust 0.9 doesn't have "Freeze" bound.
            stream_map_arc.unsafe_access(|local_stream_map| {
                let stream = stream_port.recv();
                local_stream_map.swap(peer_name.clone(), stream);
            });
        }
        
        // Enqueue the HTTP request.
        let req = HTTP_Request { peer_name: peer_name.clone(), path: ~path_obj.clone() };
        let (req_port, req_chan) = Chan::new();
        req_chan.send(req);

        debug!("Waiting for queue mutex lock.");
        req_queue_arc.access(|local_req_queue| {
            debug!("Got queue mutex lock.");
            let req: HTTP_Request = req_port.recv();
            let name = req.peer_name.clone();
            local_req_queue.push(req);
            //debug!("Priority of new request is {:d}", getPriority(name.clone()));
            debug!("A new request enqueued, now the length of queue is {:u}.", local_req_queue.len());
        });
        
        notify_chan.send(()); // Send incoming notification to responder task.
    
    
    }
    
    // TODO: Smarter Scheduling.
    fn dequeue_static_file_request(&mut self) {
        let req_queue_get = self.request_queue_arc.clone();
        let stream_map_get = self.stream_map_arc.clone();
        
        // Port<> cannot be sent to another task. So we have to make this task as the main task that can access self.notify_port.
        
        let (request_port, request_chan) = Chan::new();
        loop {
            self.notify_port.recv();    // waiting for new request enqueued.
            
            req_queue_get.access( |req_queue| {
                match req_queue.maybe_pop() { // Priority queue.
                    None => { /* do nothing */ }
                    Some(req) => {
                        request_chan.send(req);
                        debug!("A new request dequeued, now the length of queue is {:u}.", req_queue.len());
                    }
                }
            });
            
            let request = request_port.recv();
            
            // Get stream from hashmap.
            // Use unsafe method, because TcpStream in Rust 0.9 doesn't have "Freeze" bound.
            let (stream_port, stream_chan) = Chan::new();
            unsafe {
                stream_map_get.unsafe_access(|local_stream_map| {
                    let stream = local_stream_map.pop(&request.peer_name).expect("no option tcpstream");
                    stream_chan.send(stream);
                });
            }

            // caching - find out if file is smaller than 1 MB
            //if(fs::stat(request.path).size < 1048576) {
                
            //}
/*
            let (c_port, c_chan) = Chan::new();
            let (f_port, f_chan) = Chan::new();
            let cache = &self.cache_arc.access(|local_cache_queue| {
                    c_chan.send(local_cache_queue);
                });
            let file = &self.file_arc.access(|local_file_queue| {

                    f_chan.send(local_file_queue);
                });

            let cache_arc = self.cache_arc.clone();
            let file_arc = self.file_arc.clone();*/
            //let stream_map_arc = self.stream_map_arc.clone();*/
            //let (c_port, c_chan) = Chan::new();
            //c_chan.send(self.cache.to_owned());
            // TODO: Spawning more tasks to respond the dequeued requests concurrently. You may need a semophore to control the concurrency.
            //spawn(proc() {
                let stream = stream_port.recv();
                //let cache = c_port.recv();
                WebServer::respond_with_static_file(stream, request.path, self.cache.clone());
                // Close stream automatically.
                debug!("=====Terminated connection from [{:s}].=====", request.peer_name);
            //});
        }
    }
    
    fn get_peer_name(stream: &mut Option<std::io::net::tcp::TcpStream>) -> ~str {
        match *stream {
            Some(ref mut s) => {
                         match s.peer_name() {
                            Some(pn) => {pn.to_str()},
                            None => (~"")
                         }
                       },
            None => (~"")
        }
    }
}

fn get_args() -> (~str, uint, ~str) {
    fn print_usage(program: &str) {
        println!("Usage: {:s} [options]", program);
        println!("--ip     \tIP address, \"{:s}\" by default.", IP);
        println!("--port   \tport number, \"{:u}\" by default.", PORT);
        println!("--www    \tworking directory, \"{:s}\" by default", WWW_DIR);
        println("-h --help \tUsage");
    }
    
    /* Begin processing program arguments and initiate the parameters. */
    let args = os::args();
    let program = args[0].clone();
    
    let opts = ~[
        getopts::optopt("ip"),
        getopts::optopt("port"),
        getopts::optopt("www"),
        getopts::optflag("h"),
        getopts::optflag("help")
    ];

    let matches = match getopts::getopts(args.tail(), opts) {
        Ok(m) => { m }
        Err(f) => { fail!(f.to_err_msg()) }
    };

    if matches.opt_present("h") || matches.opt_present("help") {
        print_usage(program);
        unsafe { libc::exit(1); }
    }
    
    let ip_str = if matches.opt_present("ip") {
                    matches.opt_str("ip").expect("invalid ip address?").to_owned()
                 } else {
                    IP.to_owned()
                 };
    
    let port:uint = if matches.opt_present("port") {
                        from_str::from_str(matches.opt_str("port").expect("invalid port number?")).expect("not uint?")
                    } else {
                        PORT
                    };
    
    let www_dir_str = if matches.opt_present("www") {
                        matches.opt_str("www").expect("invalid www argument?") 
                      } else { WWW_DIR.to_owned() };
    
    (ip_str, port, www_dir_str)
}

fn main() {
    let (ip_str, port, www_dir_str) = get_args();
    let mut zhtta = WebServer::new(ip_str, port, www_dir_str);
    zhtta.run();
}

fn getPriority(other: ~str) -> int{
        if(other.slice_to(7) == "128.143." || other.slice_to(6) == "137.54." || other.slice_to(9) == "127.0.0.1") {
            //debug!("{:s} Piority: 1", other);
            return 1;
        } else {
            return 2;
        }
    }