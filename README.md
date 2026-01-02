
# Automated Marketing System

A Rust command-line tool that crawls directory listings (e.g., Yellow Pages US/AU), extracts business websites and contact emails, optionally generates a visual mockup and sends a marketing email with an inline image. It demonstrates web scraping, DOM parsing, headless browser rendering, and SMTP email sending.

**Repository:** https://github.com/aryansrao/automated-marketing

---

## Table of Contents

- [Overview](#overview)
- [Features](#features)
- [Architecture](#architecture)
- [Prerequisites](#prerequisites)
- [Installation](#installation)
- [Configuration (.env)](#configuration-env)
- [Build & Run](#build--run)
- [Usage](#usage)
- [How It Works](#how-it-works)
- [Email & Mockup Generation](#email--mockup-generation)
- [Persistence & Output](#persistence--output)
- [Testing](#testing)
- [Troubleshooting](#troubleshooting)
- [Security & Privacy](#security--privacy)
- [Contributing](#contributing)

---

## Overview

`automated-marketing` is a Rust-based command-line tool that crawls directory listings (e.g., Yellow Pages US/AU), extracts business websites and contact emails and generates a mockup image for each matched business, and sends a marketing email containing the mockup. The project demonstrates web scraping, HTML parsing, headless browser rendering, and SMTP email sending in Rust.

This repository contains a single binary implemented in `src/main.rs`.

## Features

- Configurable search targets and limits for US and Australia Yellow Pages.
- Concurrent, cancellable run loop (press `q` + Enter to stop).
- Email extraction using a safe regular expression with filtering to avoid false positives.
- Optional email automation with HTML content and an inline image attachment.
- Mockup generation via a headless Chrome instance (screenshotting a locally rendered HTML mockup).
- Leads exported to CSV `leads.csv` at the end of a run.

## Architecture

- `AutomatedMarketing` struct: manages HTTP client, email regex, stop flag, and email config.
- Scraping: `reqwest` for HTTP, `scraper` for DOM querying.
- Mockup generation: `headless_chrome` to render HTML and capture PNG screenshots.
- Emailing: `lettre` for SMTP send with inline image attachments.
- Persistence: CSV file output via `save_to_csv()`.

See the main implementation at [src/main.rs](src/main.rs).

## Prerequisites

- Rust toolchain (stable) and Cargo installed. See https://www.rust-lang.org/tools/install
- Google Chrome or Chromium installed (required by `headless_chrome`).
- Network access for web scraping and SMTP (if using email automation).

## Installation

Clone the repository and build with Cargo:

```bash
git clone https://github.com/aryansrao/automated-marketing.git
cd automated-marketing
cargo build --release
```

Or run it directly in dev mode:

```bash
cargo run --release
```

## Configuration (.env)

The program can optionally send emails. To enable email sending, create a `.env` file in the project root with the following variables. The program reads these variables via `dotenv`.

```
SENDER_EMAIL=you@example.com
SENDER_PASSWORD=your_smtp_password_or_app_specific_password
SENDER_NAME=Your Name or Company
PHONE_NUMBER=+1234567890
```

Notes:
- For Gmail, you should create an App Password or configure the account to allow SMTP with the credentials you provide. The code uses Gmail's SMTP server (`smtp.gmail.com` with STARTTLS on port 587).
- If `.env` is missing or variables are not set, email automation is disabled and the program will only collect leads and produce `leads.csv`.

## Build & Run

To build:

```bash
cargo build --release
```

To run (release build recommended):

```bash
./target/release/automated-marketing
```

Or using Cargo directly:

```bash
cargo run --release
```

Operation:
- The process prints status for each search and each found lead.
- Press `q` and Enter at any time to request a graceful stop.

Output:
- `leads.csv` is written to the working directory when the process completes or is stopped.

## Usage

1. Optionally configure `.env` with SMTP credentials if you want email automation.
2. Run the binary.
3. Monitor console output for found leads and email sending status.
4. When finished (or stopped via `q`), check `leads.csv` for all collected leads.

Example run (no email):

```bash
cargo run --release
```

Example run (with email):

```bash
# ensure .env is present and configured
cargo run --release
```

## How It Works

High-level flow in `src/main.rs`:

1. Build an HTTP client with a timeout and UA string.
2. Compile an email regular expression for extraction and prepare an optional `EmailConfig` from environment.
3. `get_all_searches()` returns search URLs for US and AU Yellow Pages with categories and pages.
4. For each search URL, `search_directory()` fetches the page and extracts business names/website links.
5. For each site, `scrape_business()` fetches the business site and extracts emails from the HTML.
6. If an email is found and email automation is enabled, `generate_mockup_image()` renders a simple HTML mockup and captures a PNG via `headless_chrome`.
7. `send_email()` constructs a multipart HTML email with inline PNG and sends via SMTP using `lettre`.
8. Leads are collected into memory and written to `leads.csv` at the end.

Key code locations:
- Main program: [src/main.rs](src/main.rs)
- Entry point: `fn main()` (Tokio runtime and stop-signal loop)

## Email & Mockup Generation

- The mockup is generated by writing a temporary `mockup.html` to the system temp directory and using `headless_chrome` to open and screenshot the mockup element with id `#mockup`.
- The screenshot is then attached inline to the email as a PNG (`cid:mockup-image`).
- The email uses `lettre` with STARTTLS to `smtp.gmail.com:587` by default.

Caveats:
- `headless_chrome` requires a compatible Chrome/Chromium binary on PATH. If Chrome is missing, mockup generation will fail.
- Running a headless browser is resource-intensive; consider limiting concurrency or running on a machine with sufficient memory and CPU.

## Persistence & Output

- The CSV output `leads.csv` contains a header and rows with `company_name,company_type,country,website,email`.
- CSV fields containing quotes are escaped by doubling quotes.

## Testing

There are no unit tests included in the repo currently. To perform basic verification manually:

1. Run the program without `.env` and confirm it discovers leads and writes `leads.csv`.
2. Add a `.env` with valid SMTP credentials and run again to verify emails are attempted and sent.

If you want tests added, open an issue or provide guidance on which components to cover (parsing, email validation, CSV output, etc.).

## Troubleshooting

- If scraping returns zero results for a site, check network connectivity and possible site blocking or changes to Yellow Pages DOM structure.
- If `headless_chrome` fails to launch, ensure Chrome/Chromium is installed and accessible, and that you have necessary permissions.
- If email sending fails, inspect the console error printed by the program. Common causes: invalid credentials, blocked SMTP by provider, or missing STARTTLS support.
- If you see many false-positive emails, update the email regex in `src/main.rs` and refine `is_valid_email()` filtering.

## Security & Privacy

- This tool collects publicly available emails from business websites. Respect local laws and anti-spam regulations (e.g., CAN-SPAM, GDPR). Use responsible, opt-in practices.
- Store SMTP credentials securely and avoid committing `.env` to source control. Consider using secrets management for production usage.

## Contributing

Contributions are welcome. Suggested improvements:

- Add unit and integration tests for parsing and email filters.
- Make search targets configurable via a config file or command-line flags.
- Add rate-limiting and retries with backoff for robustness.
- Improve mockup template and allow HTML customization per campaign.

To contribute:

1. Fork the repository.
2. Create a feature branch.
3. Open a pull request with a clear description of changes.

