# create by mayumi v.1
"""
NOte add lib re
Diubah untuk menggunakan asyncio dan aiohttp
Versi ini mengasumsikan penggunaan HTTPS untuk koneksi ke server proxy.
"""
import asyncio
import aiohttp
import json
import re

IP_RESOLVER = "speed.cloudflare.com"
PATH_RESOLVER = "/meta"
PROXY_FILE = "Data/ProxyIsp.txt"
OUTPUT_FILE = "Data/alive.txt"
MAX_CONCURRENT_CHECKS = 100  # Jumlah pemeriksaan proxy konkuren, sesuaikan sesuai kebutuhan
REQUEST_TIMEOUT = 10  # Waktu timeout untuk setiap permintaan dalam detik

active_proxies = []  # List untuk menyimpan proxy aktif

def clean_org_name(org_name): # Menghapus karakter yang tidak diinginkan dari nama organisasi.
    return re.sub(r'[^a-zA-Z0-9\s]', '', org_name) if org_name else org_name

async def check_ip_via_proxy(session, target_url, proxy_url=None):
    """
    Melakukan koneksi ke URL target, opsional melalui proxy, dan mengambil respons JSON.
    """
    headers = {
        "User-Agent": "Mozilla/5.0 (Windows NT 10.0) AppleWebKit/537.36 "
                      "(KHTML, like Gecko) Chrome/42.0.2311.135 Safari/537.36 Edge/12.10240",
        "Accept": "application/json, */*",
    }
    
    try:
        async with session.get(target_url, headers=headers, proxy=proxy_url, timeout=aiohttp.ClientTimeout(total=REQUEST_TIMEOUT)) as response:
            if response.status == 200:
                try:
                    return await response.json(content_type=None)
                except json.JSONDecodeError:
                    print(f"Peringatan: Gagal parse JSON langsung dari {target_url} (proxy: {proxy_url}). Mencoba baca teks.")
                    text_data = await response.text()
                    try:
                        return json.loads(text_data)
                    except json.JSONDecodeError as e_text:
                        print(f"Error parsing JSON dari teks {target_url} (proxy: {proxy_url}): {e_text}")
                except aiohttp.client_exceptions.ContentTypeError as e_content:
                    print(f"Error ContentType saat meminta {target_url} (proxy: {proxy_url}): {e_content}. Mencoba baca teks.")
                    text_data = await response.text()
                    try:
                        return json.loads(text_data)
                    except json.JSONDecodeError as e_text_ct:
                        print(f"Error parsing JSON dari teks (setelah ContentTypeError) {target_url} (proxy: {proxy_url}): {e_text_ct}")
            else:
                print(f"Error status {response.status} saat meminta {target_url} (proxy: {proxy_url})")

    except aiohttp.ClientError:
        pass
    except asyncio.TimeoutError:
        pass
    except Exception as e:
        print(f"Error tidak diketahui saat check {target_url} (proxy: {proxy_url}): {e}")
        
    return {}

async def process_proxy(session, proxy_line, semaphore):
    """
    Memproses satu baris proxy, memeriksa keaktifannya, dan menambahkannya ke daftar jika aktif.
    """
    async with semaphore:
        proxy_line = proxy_line.strip()
        if not proxy_line:
            return

        try:
            parts = proxy_line.split(",")
            if len(parts) < 2:
                print(f"Format baris proxy tidak valid (kurang dari 2 bagian): {proxy_line}. Lewati.")
                return

            ip = parts[0]
            port = parts[1]
            country_from_file = parts[2] if len(parts) > 2 else ""
            org_from_file = parts[3] if len(parts) > 3 else ""

            try:
                port_num = int(port)
            except ValueError:
                print(f"Port tidak valid untuk proxy {ip}:{port}. Lewati.")
                return

            target_url = f"https://{IP_RESOLVER}{PATH_RESOLVER}" # Target selalu HTTPS

            # --- PERUBAHAN DI SINI ---
            # Mengasumsikan proxy adalah HTTPS proxy.
            # Jika proxy Anda adalah HTTP proxy standar, gunakan "http://"
            proxy_url_for_aiohttp = f"https://{ip}:{port_num}"
            # -------------------------

            ori_data = await check_ip_via_proxy(session, target_url)
            pxy_data = await check_ip_via_proxy(session, target_url, proxy_url=proxy_url_for_aiohttp)

            if ori_data and pxy_data and ori_data.get("clientIp") != pxy_data.get("clientIp"):
                org_name_from_check = clean_org_name(pxy_data.get("asOrganization"))
                proxy_entry = f"{ip},{port},{country_from_file},{org_name_from_check}"
                print(f"CF PROXY LIVE (via HTTPS Proxy)!: {proxy_entry}")
                active_proxies.append(proxy_entry)

        except ValueError:
            print(f"Format baris proxy tidak valid: {proxy_line}. Pastikan formatnya ip,port,country,org")
        except Exception as e:
            print(f"Error saat memproses proxy {proxy_line}: {e}")


async def main():
    try:
        with open(OUTPUT_FILE, "w") as f:
            f.write("")
        print(f"File {OUTPUT_FILE} telah dikosongkan sebelum proses scan dimulai.")
    except IOError as e:
        print(f"Error saat mengosongkan file {OUTPUT_FILE}: {e}")
        return

    try:
        with open(PROXY_FILE, "r") as f:
            proxies_lines = f.readlines()
    except FileNotFoundError:
        print(f"File tidak ditemukan: {PROXY_FILE}")
        return
    except IOError as e:
        print(f"Error saat membaca file {PROXY_FILE}: {e}")
        return

    if not proxies_lines:
        print(f"Tidak ada proxy ditemukan di {PROXY_FILE}.")
        return

    semaphore = asyncio.Semaphore(MAX_CONCURRENT_CHECKS)
    
    async with aiohttp.ClientSession() as session:
        print(f"Memulai pengecekan {len(proxies_lines)} proxy dengan asumsi HTTPS PROXY, maks {MAX_CONCURRENT_CHECKS} koneksi konkuren...")
        
        tasks = [process_proxy(session, line, semaphore) for line in proxies_lines]
        await asyncio.gather(*tasks)

    if active_proxies:
        try:
            with open(OUTPUT_FILE, "w") as f_me:
                f_me.write("\n".join(active_proxies) + "\n")
            print(f"Semua {len(active_proxies)} proxy aktif disimpan ke {OUTPUT_FILE}")
        except IOError as e:
            print(f"Error saat menulis ke file {OUTPUT_FILE}: {e}")
    else:
        print(f"Tidak ada proxy aktif yang ditemukan untuk disimpan.")

    print("Pengecekan proxy selesai.")


if __name__ == "__main__":
    asyncio.run(main())
  
