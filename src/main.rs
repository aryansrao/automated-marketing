use reqwest::Client;
use scraper::{Html, Selector, ElementRef};
use std::time::Duration;
use tokio;
use regex::Regex;
use std::collections::HashSet;
use std::fs::File;
use std::io::{Write, stdin};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use headless_chrome::{Browser, LaunchOptions};
use lettre::message::{header, MultiPart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use anyhow::Result;
use std::thread;
use std::env;
use indicatif::{ProgressBar, ProgressStyle};
use colored::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CompanyLead {
    pub company_name: String,
    pub company_type: String,
    pub country: String,
    pub website: String,
    pub email: String,
}

struct EmailConfig {
    sender_email: String,
    sender_password: String,
    sender_name: String,
    phone_number: String,
}

impl EmailConfig {
    fn from_env() -> Result<Self> {
        dotenv::dotenv().ok();
        
        Ok(Self {
            sender_email: env::var("SENDER_EMAIL")?,
            sender_password: env::var("SENDER_PASSWORD")?,
            sender_name: env::var("SENDER_NAME")?,
            phone_number: env::var("PHONE_NUMBER")?,
        })
    }
}

pub struct AutomatedMarketing {
    client: Client,
    email_regex: Regex,
    should_stop: Arc<AtomicBool>,
    email_config: Option<EmailConfig>,
}

impl AutomatedMarketing {
    pub fn new(should_stop: Arc<AtomicBool>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .expect("Failed to build HTTP client");

        let email_regex = Regex::new(
            r"\b[a-zA-Z0-9][a-zA-Z0-9._%+-]{0,63}@[a-zA-Z0-9][a-zA-Z0-9.-]*\.[a-zA-Z]{2,}\b"
        ).unwrap();

        let email_config = EmailConfig::from_env().ok();

        Self { 
            client, 
            email_regex, 
            should_stop, 
            email_config,
        }
    }

    fn get_all_searches(&self) -> Vec<(String, String, String)> {
        let mut searches = Vec::new();
        
        let us_cities = vec![
            ("New+York+NY", "New York"), ("Los+Angeles+CA", "Los Angeles"),
            ("Chicago+IL", "Chicago"), ("Houston+TX", "Houston"),
            ("Phoenix+AZ", "Phoenix"), ("Philadelphia+PA", "Philadelphia"),
        ];
        
        let us_categories = vec![
            ("restaurants", "Restaurant"), ("plumbers", "Plumbing"),
            ("electricians", "Electrical"), ("contractors", "Construction"),
            ("lawyers", "Legal"), ("dentists", "Dental"),
        ];
        
        for (city_code, _) in us_cities {
            for (category, biz_type) in &us_categories {
                for page in 1..=5 {
                    searches.push((
                        format!("https://www.yellowpages.com/search?search_terms={}&geo_location_terms={}&page={}", 
                            category, city_code, page),
                        biz_type.to_string(),
                        "USA".to_string()
                    ));
                }
            }
        }
        
        let au_cities = vec!["Sydney+NSW", "Melbourne+VIC", "Brisbane+QLD"];
        let au_categories = vec![("restaurants", "Restaurant"), ("cafes", "Cafe")];
        
        for city in au_cities {
            for (category, biz_type) in &au_categories {
                for page in 1..=3 {
                    searches.push((
                        format!("https://www.yellowpages.com.au/search/listings?clue={}&locationClue={}&pageNumber={}", 
                            category, city, page),
                        biz_type.to_string(),
                        "Australia".to_string()
                    ));
                }
            }
        }
        
        searches
    }

    pub async fn search_directory(&self, url: &str, _business_type: &str, _country: &str) -> Vec<(String, String)> {
        if self.should_stop.load(Ordering::Relaxed) {
            return Vec::new();
        }
        
        tokio::time::sleep(Duration::from_millis(2000)).await;
        
        match self.client.get(url).send().await {
            Ok(response) => {
                match response.text().await {
                    Ok(body) => {
                        let document = Html::parse_document(&body);
                        self.extract_businesses(&document, url)
                    }
                    Err(_) => Vec::new()
                }
            }
            Err(_) => Vec::new()
        }
    }

    fn extract_businesses(&self, document: &Html, base_url: &str) -> Vec<(String, String)> {
        let mut businesses = Vec::new();
        
        if base_url.contains("yellowpages.com") && !base_url.contains(".au") {
            businesses.extend(self.extract_from_us_yellowpages(document));
        } else if base_url.contains("yellowpages.com.au") {
            businesses.extend(self.extract_from_au_yellowpages(document));
        }
        
        businesses.into_iter().take(30).collect()
    }

    fn extract_from_us_yellowpages(&self, document: &Html) -> Vec<(String, String)> {
        let mut businesses = Vec::new();
        
        if let Ok(result_selector) = Selector::parse("div.result") {
            for result in document.select(&result_selector) {
                let name = self.extract_business_name_us(result);
                let website = self.extract_website_url_us(result);
                
                if let (Some(n), Some(w)) = (name, website) {
                    if !n.is_empty() && w.starts_with("http") && self.is_valid_business_url(&w) {
                        businesses.push((w, n));
                    }
                }
            }
        }
        
        businesses
    }

    fn extract_business_name_us(&self, result: ElementRef) -> Option<String> {
        let name_selectors = vec!["a.business-name", "h2.n", "span.business-name"];

        for selector_str in name_selectors {
            if let Ok(selector) = Selector::parse(selector_str) {
                if let Some(element) = result.select(&selector).next() {
                    let text: String = element.text().collect::<String>().trim().to_string();
                    if text.len() > 2 && text.len() < 150 {
                        return Some(text);
                    }
                }
            }
        }
        None
    }

    fn extract_website_url_us(&self, result: ElementRef) -> Option<String> {
        if let Ok(selector) = Selector::parse("a[href*='http']") {
            for element in result.select(&selector) {
                if let Some(href) = element.value().attr("href") {
                    if href.starts_with("http") && self.looks_like_company_website(href) {
                        return Some(href.to_string());
                    }
                }
            }
        }
        None
    }

    fn extract_from_au_yellowpages(&self, document: &Html) -> Vec<(String, String)> {
        let mut businesses = Vec::new();
        
        if let Ok(listing_selector) = Selector::parse("div.listing-item, article.listing") {
            for listing in document.select(&listing_selector) {
                let name = self.extract_text_from_selectors(listing, &["h3.listing-name a", "a.listing-name"]);
                let website = self.extract_website_from_listing(listing);
                
                if let (Some(n), Some(w)) = (name, website) {
                    if !n.is_empty() && w.starts_with("http") && self.is_valid_business_url(&w) {
                        businesses.push((w, n));
                    }
                }
            }
        }
        
        businesses
    }

    fn extract_text_from_selectors(&self, element: ElementRef, selectors: &[&str]) -> Option<String> {
        for selector_str in selectors {
            if let Ok(selector) = Selector::parse(selector_str) {
                if let Some(elem) = element.select(&selector).next() {
                    let text: String = elem.text().collect::<String>().trim().to_string();
                    if text.len() > 2 && text.len() < 150 {
                        return Some(text);
                    }
                }
            }
        }
        None
    }

    fn extract_website_from_listing(&self, listing: ElementRef) -> Option<String> {
        if let Ok(selector) = Selector::parse("a[href*='http']") {
            for element in listing.select(&selector) {
                let text = element.text().collect::<String>().to_lowercase();
                
                if text.contains("website") || text.contains("visit") {
                    if let Some(href) = element.value().attr("href") {
                        if href.starts_with("http") && self.looks_like_company_website(href) {
                            return Some(href.to_string());
                        }
                    }
                }
            }
        }
        None
    }

    fn is_valid_business_url(&self, url: &str) -> bool {
        !url.contains("facebook.com") && !url.contains("thryv.com") && url.len() > 20
    }

    fn looks_like_company_website(&self, url: &str) -> bool {
        let url_lower = url.to_lowercase();
        !url_lower.contains("yellowpages") && !url_lower.contains("yelp.com") && url_lower.starts_with("http")
    }

    pub async fn scrape_business(&self, url: &str, name: &str, business_type: &str, country: &str) -> Option<CompanyLead> {
        if self.should_stop.load(Ordering::Relaxed) {
            return None;
        }
        
        tokio::time::sleep(Duration::from_millis(800)).await;
        
        let response = self.client.get(url).send().await.ok()?;
        let body = response.text().await.ok()?;
        
        let emails = self.extract_emails(&body);
        
        if emails.is_empty() {
            return None;
        }

        Some(CompanyLead {
            company_name: name.to_string(),
            company_type: business_type.to_string(),
            country: country.to_string(),
            website: url.to_string(),
            email: emails[0].clone(),
        })
    }

    fn extract_emails(&self, body: &str) -> Vec<String> {
        let mut emails = HashSet::new();
        
        for email_match in self.email_regex.find_iter(body) {
            let email = email_match.as_str().to_lowercase();
            
            if self.is_valid_email(&email) {
                emails.insert(email);
            }
        }

        let mut result: Vec<String> = emails.into_iter().collect();
        result.sort();
        result
    }

    fn is_valid_email(&self, email: &str) -> bool {
        if !email.contains('@') || email.len() < 6 {
            return false;
        }

        let bad = vec![".png", ".jpg", ".gif", ".css", ".js", "example.com", "@sentry", "noreply", "@localhost", "test@", "filler@"];
        
        for pattern in bad {
            if email.contains(pattern) {
                return false;
            }
        }

        if let Some(domain) = email.split('@').nth(1) {
            domain.contains('.') && domain.len() > 3
        } else {
            false
        }
    }

    fn generate_mockup_image(&self, company_name: &str, company_type: &str, website: &str, country: &str) -> Result<Vec<u8>> {
        let country_lower = country.to_lowercase();
        let html = format!(r#"<!DOCTYPE html>
<html><head><meta charset="UTF-8"><style>
body{{font-family:arial,sans-serif;background:#fff;margin:0;padding:0}}
#mockup{{width:1080px;background:#fff}}
.header{{padding:30px 40px 0 40px}}
.logo{{font-size:42px;margin-bottom:35px}}
.g{{color:#4285f4}}.o1{{color:#ea4335}}.o2{{color:#fbbc04}}.l{{color:#34a853}}.e{{color:#ea4335}}
.search-box{{border:1px solid #dfe1e5;border-radius:28px;padding:18px 24px;font-size:22px;box-shadow:0 1px 6px rgba(32,33,36,.28);margin-bottom:30px}}
.nav{{display:flex;padding:0 40px;gap:40px;border-bottom:1px solid #e8eaed}}
.nav-item{{padding:18px 0;font-size:18px;color:#5f6368}}
.nav-item.active{{color:#1a73e8;border-bottom:3px solid #1a73e8;margin-bottom:-1px}}
.results-info{{padding:12px 40px 20px 40px;font-size:18px;color:#70757a}}
.result{{padding:0 40px 60px 40px}}
.result-header{{display:flex;gap:18px;margin-bottom:8px}}
.favicon{{width:44px;height:44px;border-radius:50%;background:#f1f3f4;display:flex;align-items:center;justify-content:center;font-size:22px;color:#5f6368}}
.url-line{{font-size:18px;color:#5f6368;margin-bottom:4px}}
.site-name{{font-size:22px;color:#202124}}
.title{{font-size:32px;color:#1a0dab;margin:12px 0}}
.description{{font-size:20px;color:#4d5156;margin-top:12px}}
.cta{{background:#e8f0fe;border:1px solid #c2dbff;border-radius:12px;padding:24px 28px;margin-top:24px}}
.cta-title{{font-size:22px;color:#1967d2;font-weight:500;margin-bottom:12px}}
.cta-text{{font-size:19px;color:#3c4043}}
</style></head><body><div id="mockup">
<div class="header"><div class="logo"><span class="g">G</span><span class="o1">o</span><span class="o2">o</span><span class="g">g</span><span class="l">l</span><span class="e">e</span></div>
<div class="search-box">best {} in {}</div></div>
<div class="nav"><div class="nav-item active">All</div><div class="nav-item">Images</div><div class="nav-item">Shopping</div></div>
<div class="results-info">About 2,45,000 results</div>
<div class="result"><div class="result-header"><div class="favicon">{}</div><div><div class="url-line">{}</div><div class="site-name">{}</div></div></div>
<div class="title">{} - Best {} in {}</div>
<div class="description">{}'s best {} providing premium quality products and services.</div>
<div class="cta"><div class="cta-title">This could be your website - {}</div>
<div class="cta-text">Get your website now • Contact us at +917898429176 or visit luveian.com</div></div></div></div></body></html>"#,
            company_type,
            country_lower,
            company_name.chars().next().unwrap_or('C'),
            website,
            company_name,
            company_name,
            company_type,
            country,
            country,
            company_type,
            website
        );
        
        let temp_path = std::env::temp_dir().join("mockup.html");
        std::fs::write(&temp_path, html)?;

        let browser = Browser::new(LaunchOptions {
            headless: true,
            window_size: Some((1200, 1500)),
            ..Default::default()
        })?;

        let url = format!("file://{}", temp_path.display());
        let tab = browser.new_tab()?;
        tab.navigate_to(&url)?;
        thread::sleep(Duration::from_millis(2000));

        let element = tab.wait_for_element("#mockup")?;
        let screenshot = element.capture_screenshot(
            headless_chrome::protocol::cdp::Page::CaptureScreenshotFormatOption::Png,
        )?;

        let _ = std::fs::remove_file(&temp_path);
        Ok(screenshot)
    }

    fn send_email(&self, to_email: &str, company_name: &str, image_data: Vec<u8>) -> Result<()> {
        let config = self.email_config.as_ref().ok_or_else(|| anyhow::anyhow!("Email config not set"))?;
        
        let subject = format!("This could be {}", company_name);

        let html_body = format!(r#"<!DOCTYPE html><html><head><style>
body{{margin:0;padding:0;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,Arial,sans-serif;background:#f5f5f5}}
.container{{max-width:600px;margin:0 auto;background:#fff}}
.image-container img{{width:100%;max-width:600px;height:auto;display:block}}
.content{{padding:40px 30px;text-align:center}}
.title{{font-size:24px;font-weight:600;color:#1a1a1a;margin:0 0 20px 0}}
.subtitle{{font-size:16px;color:#666;margin:0 0 30px 0}}
.contact{{font-size:16px;color:#1a1a1a}}
.contact a{{color:#1a73e8;text-decoration:none;font-weight:500}}
.footer{{padding:20px 30px;text-align:center;font-size:13px;color:#999;border-top:1px solid #eee}}
</style></head><body><div class="container">
<div class="image-container"><img src="cid:mockup-image" alt="Mockup"/></div>
<div class="content"><h1 class="title">This could've been your company</h1>
<p class="subtitle">& it still could</p>
<p class="contact">Contact <a href="mailto:hello@luveian.com">hello@luveian.com</a> or <a href="tel:{}">{}</a></p></div>
<div class="footer">Luveian - Software & Marketing Solutions</div></div></body></html>"#,
            config.phone_number, config.phone_number
        );

        let email = Message::builder()
            .from(format!("{} <{}>", config.sender_name, config.sender_email).parse()?)
            .to(to_email.parse()?)
            .subject(subject)
            .multipart(
                MultiPart::related()
                    .singlepart(lettre::message::SinglePart::builder()
                        .header(header::ContentType::TEXT_HTML)
                        .body(html_body))
                    .singlepart(lettre::message::SinglePart::builder()
                        .header(header::ContentType::parse("image/png")?)
                        .header(header::ContentDisposition::inline())
                        .header(header::ContentId::from("<mockup-image>".to_string()))
                        .body(image_data))
            )?;

        let creds = Credentials::new(config.sender_email.clone(), config.sender_password.clone());
        let mailer = SmtpTransport::starttls_relay("smtp.gmail.com")?
            .credentials(creds)
            .port(587)
            .build();

        mailer.send(&email)?;
        Ok(())
    }

    pub async fn run_continuous(&self) -> Vec<CompanyLead> {
        let all_leads = Arc::new(Mutex::new(HashSet::new()));
        let searches = self.get_all_searches();
        let send_emails = self.email_config.is_some();

        println!("\n{}", "AUTOMATED MARKETING SYSTEM".bright_cyan().bold());
        println!("{}", "─".repeat(50).bright_black());
        
        if send_emails {
            println!("{} {}", "●".green(), "Email automation enabled".bright_white());
        } else {
            println!("{} {}", "○".yellow(), "Email automation disabled".bright_black());
        }
        
        println!("{} {}\n", "→".bright_blue(), "Press 'q' to stop".bright_black());

        let total_searches = searches.len();
        let mut completed = 0;

        for (search_url, business_type, country) in searches {
            if self.should_stop.load(Ordering::Relaxed) {
                break;
            }

            completed += 1;
            println!("{} Searching {} {} {}/{}",
                "→".bright_blue(),
                business_type.bright_cyan(),
                format!("in {}", country).bright_black(),
                completed.to_string().bright_yellow(),
                total_searches.to_string().bright_black()
            );

            let businesses = self.search_directory(&search_url, &business_type, &country).await;

            for (website, name) in businesses {
                if self.should_stop.load(Ordering::Relaxed) {
                    break;
                }

                if let Some(lead) = self.scrape_business(&website, &name, &business_type, &country).await {
                    let lead_count = all_leads.lock().unwrap().len() + 1;
                    
                    println!("\n  {} {} {}",
                        format!("[{}]", lead_count).bright_black(),
                        lead.company_name.bright_white().bold(),
                        format!("• {}", lead.company_type).bright_cyan()
                    );
                    println!("      {} {} {} {}",
                        format!(" {}", lead.country).bright_yellow(),
                        "•".bright_black(),
                        format!(" {}", lead.email).bright_green(),
                        "".bright_black()
                    );
                    
                    if send_emails {
                        let pb = ProgressBar::new(3);
                        pb.set_style(
                            ProgressStyle::default_bar()
                                .template("      {msg} {bar:20.green/blue} {pos}/3")
                                .unwrap()
                                .progress_chars("█▓░")
                        );
                        
                        pb.set_message("Creating mockup");
                        match self.generate_mockup_image(&lead.company_name, &lead.company_type, &lead.website, &lead.country) {
                            Ok(image) => {
                                pb.inc(1);
                                pb.set_message("Preparing email");
                                thread::sleep(Duration::from_millis(300));
                                pb.inc(1);
                                pb.set_message("Sending email  ");
                                
                                match self.send_email(&lead.email, &lead.company_name, image) {
                                    Ok(_) => {
                                        pb.inc(1);
                                        pb.finish_with_message(format!("{}", "✓ Email sent     ".bright_green()));
                                    }
                                    Err(_) => {
                                        pb.finish_with_message(format!("{}", "✗ Send failed    ".bright_red()));
                                    }
                                }
                            }
                            Err(_) => {
                                pb.finish_with_message(format!("{}", "✗ Mockup failed  ".bright_red()));
                            }
                        }
                        println!();
                        thread::sleep(Duration::from_secs(2));
                    }
                    
                    all_leads.lock().unwrap().insert(lead);
                }
            }
            println!();
        }
        
        let leads: Vec<_> = all_leads.lock().unwrap().iter().cloned().collect();
        leads
    }

    pub fn save_to_csv(&self, leads: &[CompanyLead], filename: &str) -> std::io::Result<()> {
        let mut file = File::create(filename)?;
        
        writeln!(file, "company_name,company_type,country,website,email")?;
        
        for lead in leads {
            writeln!(file, "\"{}\",\"{}\",\"{}\",\"{}\",\"{}\"",
                lead.company_name.replace('"', "\"\""),
                lead.company_type.replace('"', "\"\""),
                lead.country,
                lead.website,
                lead.email
            )?;
        }
        
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let should_stop = Arc::new(AtomicBool::new(false));
    let should_stop_clone = should_stop.clone();

    tokio::spawn(async move {
        let mut input = String::new();
        loop {
            input.clear();
            if stdin().read_line(&mut input).is_ok() {
                if input.trim().to_lowercase() == "q" {
                    should_stop_clone.store(true, Ordering::Relaxed);
                    break;
                }
            }
        }
    });

    let system = AutomatedMarketing::new(should_stop.clone());
    let leads = system.run_continuous().await;

    println!("\n{}", "─".repeat(50).bright_black());
    println!("{} {}", "✓".bright_green(), format!("Collected {} leads", leads.len()).bright_white().bold());

    let filename = "leads.csv";
    match system.save_to_csv(&leads, filename) {
        Ok(_) => println!("{} {}", "✓".bright_green(), format!("Saved to {}", filename).bright_white()),
        Err(e) => println!("{} {}", "✗".bright_red(), format!("Error: {}", e)),
    }

    println!("{}\n", "─".repeat(50).bright_black());

    Ok(())
}
