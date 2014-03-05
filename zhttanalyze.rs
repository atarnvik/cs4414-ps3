
extern mod extra;
extern mod http;


use std::os;
use std::io::File;
use std::io::buffered::BufferedReader;
use std::str;

use http::client::RequestWriter;
use http::method::Get;
use http::status;

use extra::treemap::TreeMap;

static REQUEST_STUB : &'static str = "http://ip-api.com/csv/";

fn main() {
	//read in file into array of IP addresses without port info after colon
	let path = Path::new(os::args()[1]);
	if path.exists() {
		//check if file exists
		let mut IPAddressMap: TreeMap<~str,int> = TreeMap::new();
		let mut input_file_reader = BufferedReader::new(File::open(&path));
		for line in input_file_reader.lines() {
			let IPAddress : ~str= line.splitn(':', 1).nth(0).expect("no program").to_owned();
			match (IPAddressMap.pop(&IPAddress)) {
				Some(count) => {
					IPAddressMap.swap(IPAddress.clone(),count+1);
				},
				None => {
					IPAddressMap.swap(IPAddress.clone(),1);
				}
			};
		};

		//get zip codes and put them in map
		let mut ZipMap: TreeMap<~str,int> = TreeMap::new();
		for nextPair in IPAddressMap.iter() {
			match(nextPair) {
				(ip,IPCount) => {
					let request = RequestWriter::new(Get, from_str(REQUEST_STUB + *ip).expect("Invalid URL"));
				    let mut response = match request.read_response() {
				        Ok(response) => response,
				        Err(_request) => unreachable!(), // Uncaught condition will have failed first
				    };
				    if response.status == status::Ok {
				    	let body = response.read_to_end().to_owned();
				    	let bodystr: ~str = str::from_utf8(body).to_owned();
				    	let mut location_parts = bodystr.split(',');
				    	let mut location : ~str = ~"";
				    	if (*ip == ~"127.0.0.1") {
				    		location = ~"Internal";
				    	} else {
					    	let country : ~str = location_parts.nth(1).expect("country split").to_owned();
					    	let state: ~str = location_parts.nth(2).expect("state split").to_owned();
					    	let zip: ~str = location_parts.nth(1).expect("zip split").to_owned();
					    	location = country.slice(1,country.len()-1) + "\t\t" + state + "\t" + zip;
					    }
			    		match (ZipMap.pop(&location)) {
							Some(count) => {
								ZipMap.swap(location.clone(),count+*IPCount);
							},
							None => {
								ZipMap.swap(location.clone(),*IPCount);
							}
						};
				    } else {
				        println!("That URL ain't returning 200 OK, it returned {} instead", response.status);
				    }
				},
				
			};
		}

		//print individual zips and count
		println("Count\tCountry\t\t\tState\t\tZip Code");
		println("-----\t-----\t\t\t-----\t\t-----");

		for nextPair in ZipMap.iter() {
			match(nextPair) {
				(location,count) => {
					println!("{:d}\t{:s}",*count,*location);
				},
				
			};
		}

 	} else {
		println!("No file iplog.txt found.");
	}
	//go through array of IP addresses, finding each individal instance and its frequency
	//go through that array and get a zip code and state for each
	//go through zip codes and add frequency
	//sort based on frequency
	//print out info
}

