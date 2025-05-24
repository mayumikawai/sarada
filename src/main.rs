
use tokio::fs::File;
use tokio::io::{self, AsyncBufReadExt, BufReader};
use reqwest::{Client, Proxy, Error};
use serde::{Deserialize, Serialize};
use rayon::prelude::*;
// use std::collections::HashMap; // BARIS INI DIHAPUS
use std::sync::{Arc, Mutex};
use std::time::Duration;
use regex::Regex;
use futures::future::join_all;

const CLOUDFLARE_META_URL: &str = "https://speed.cloudflare.com/meta";
const PROXY_TIMEOUT_SECONDS: u64 = 10; // Timeout untuk setiap permintaan proxy

#[derive(Debug, Deserialize, Serialize)]
struct CloudflareMeta {
    ip: String,
    country: Option<String>,
    #[serde(rename = "asOrganization")]
    as_organization: Option<String>,
}

/// Membersihkan string dari karakter non-alfanumerik atau non-spasi.
fn clean_string(s: &str) -> String {
    let re = Regex::new(r"[^a-zA-Z0-9\s.,\-_()]").unwrap();
    re.replace_all(s, "").trim().to_string()
}

/// Mengambil metadata Cloudflare (termasuk IP) baik tanpa proxy maupun dengan proxy.
async fn get_cloudflare_meta(client: &Client) -> Result<CloudflareMeta, Error> {
    let response = client.get(CLOUDFLARE_META_URL)
        .timeout(Duration::from_secs(PROXY_TIMEOUT_SECONDS))
        .send()
        .await?
        .json::<CloudflareMeta>()
        .await?;
    Ok(response)
}

/// Memproses satu proxy: mendapatkan IP asli, mencoba IP melalui proxy, dan membandingkan.
async fn process_proxy(
    proxy_addr: String,
    original_ip: Arc<String>,
    alive_proxies: Arc<Mutex<Vec<String>>>,
) {
    println!("Mengecek proxy: {}", proxy_addr);

    let client_builder = Client::builder()
        .timeout(Duration::from_secs(PROXY_TIMEOUT_SECONDS))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36");

    let proxy_obj = match Proxy::all(&proxy_addr) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error membuat objek proxy {}: {}", proxy_addr, e);
            return;
        }
    };

    let client_with_proxy = match client_builder.proxy(proxy_obj).build() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error membuat client dengan proxy {}: {}", proxy_addr, e);
            return;
        }
    };

    match get_cloudflare_meta(&client_with_proxy).await {
        Ok(meta_with_proxy) => {
            if meta_with_proxy.ip != *original_ip {
                // Gunakan .clone() pada Option sebelum unwrap_or_else()
                let country = meta_with_proxy.country.clone().unwrap_or_else(|| "N/A".to_string());
                let org = meta_with_proxy.as_organization.clone().unwrap_or_else(|| "N/A".to_string());

                let cleaned_country = clean_string(&country);
                let cleaned_org = clean_string(&org);

                let alive_entry = format!(
                    "Proxy Aktif: {} | IP: {} | Negara: {} | Organisasi: {}",
                    proxy_addr, meta_with_proxy.ip, cleaned_country, cleaned_org
                );
                println!("DITEMUKAN: {}", alive_entry);
                alive_proxies.lock().unwrap().push(alive_entry);
            } else {
                println!("Proxy mati (IP tidak berubah): {}", proxy_addr);
            }
        }
        Err(e) => {
            eprintln!("Gagal mengakses Cloudflare melalui proxy {}: {}", proxy_addr, e);
        }
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    println!("Memulai pemeriksaan proxy...");

    // 1. Dapatkan IP asli perangkat
    let client_without_proxy = Client::builder()
        .timeout(Duration::from_secs(PROXY_TIMEOUT_SECONDS))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()
        .expect("Gagal membangun client tanpa proxy");

    println!("Mendapatkan IP asli perangkat...");
    let original_meta = match get_cloudflare_meta(&client_without_proxy).await {
        Ok(meta) => {
            // Gunakan .clone() pada Option sebelum unwrap_or_else()
            let country = meta.country.clone().unwrap_or_else(|| "N/A".to_string());
            let org = meta.as_organization.clone().unwrap_or_else(|| "N/A".to_string());
            println!(
                "IP Asli Perangkat: {} | Negara: {} | Organisasi: {}",
                meta.ip,
                clean_string(&country),
                clean_string(&org)
            );
            meta // Kembalikan seluruh struct meta, yang sekarang masih utuh
        }
        Err(e) => {
            eprintln!("Gagal mendapatkan IP asli perangkat dari Cloudflare: {}", e);
            eprintln!("Pastikan Anda memiliki koneksi internet aktif.");
            return Ok(());
        }
    };
    let original_ip = Arc::new(original_meta.ip);

    // 2. Baca daftar proxy dari file (misal: proxies.txt)
    let file = File::open("data/rawProxyList.txt").await?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut proxy_list = Vec::new();

    println!("Membaca daftar proxy dari proxies.txt...");
    while let Some(line) = lines.next_line().await? {
        let trimmed_line = line.trim();
        if !trimmed_line.is_empty() {
            proxy_list.push(trimmed_line.to_string());
        }
    }

    if proxy_list.is_empty() {
        println!("Tidak ada proxy ditemukan di proxies.txt. Harap isi file dengan satu proxy per baris.");
        return Ok(());
    }
    println!("Total {} proxy ditemukan.", proxy_list.len());

    // 3. Proses proxy secara paralel menggunakan Rayon dan Tokio (join_all)
    let alive_proxies: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    // Mengumpulkan semua task async ke dalam sebuah Vec
    let tasks: Vec<_> = proxy_list
        .into_par_iter() // Rayon untuk paralelisme pada iterasi
        .map(|proxy_addr| {
            let original_ip_clone = Arc::clone(&original_ip);
            let alive_proxies_clone = Arc::clone(&alive_proxies);
            tokio::spawn(process_proxy(
                proxy_addr,
                original_ip_clone,
                alive_proxies_clone,
            ))
        })
        .collect();

    // Menunggu semua task async selesai
    join_all(tasks).await;

    // 4. Simpan proxy yang aktif ke alive.txt
    let final_alive_proxies = alive_proxies.lock().unwrap();
    if final_alive_proxies.is_empty() {
        println!("Tidak ada proxy aktif yang ditemukan.");
    } else {
        let mut output_file = File::create("data/alive.txt").await?;
        for proxy_entry in final_alive_proxies.iter() {
            tokio::io::AsyncWriteExt::write_all(&mut output_file, proxy_entry.as_bytes()).await?;
            tokio::io::AsyncWriteExt::write_all(&mut output_file, b"\n").await?;
        }
        println!("{} proxy aktif telah disimpan ke alive.txt", final_alive_proxies.len());
    }

    println!("Pemeriksaan proxy selesai.");

    Ok(())
}
