# Movement Tracker

This repository contains the source code and configuration for the **Movement Tracker** application, which is built using Rust, `sqlx`, PostgreSQL, and Teloxide (for interacting with Telegram). The application tracks user movements and integrates with PostgreSQL for data storage. It is containerized using Docker and Docker Compose.

## Prerequisites

Ensure you have the following installed:

- [Docker](https://www.docker.com/)
- [Docker Compose](https://docs.docker.com/compose/)
- Rust toolchain (`cargo`) for local development

## How to use

The docker image is available at

```
ghcr.io/therealorange/movement-tracker:release
```
Specify the prerequisite environment variables in order to use this application (you must have a PostgreSQL database up!). 
Refer to the `docker-compose.yml` file for an example of how one may run the application with a database in a Compose stack. 
(be sure to replace the `build:` directive with the `image:` directive)

## Building

### 1. Clone the repository

```bash
git clone <repository-url>
cd movement_tracker
```

### 2. Environment Configuration

Copy the provided `.env.example` file to `.env` and update the values as necessary.

```bash
cp .env.example .env
```

#### Example `.env` file:

```dotenv
TELOXIDE_TOKEN=xxx
RUST_LOG=info
MAX_DB_CONNECTIONS=50

POSTGRES_DB=db
POSTGRES_USER=user
POSTGRES_PASSWD=password

DEFAULT_TELEGRAM_ID=<specify the default telegram id here>
DEFAULT_USER_NAME="John Doe"
DEFAULT_OPS_NAME="JOHN D"

# Used only for running the application standalone from the docker compose
POSTGRES_PORT=5552
POSTGRES_URL=127.0.0.1
DATABASE_URL="postgresql://${POSTGRES_USER}:${POSTGRES_PASSWD}@${POSTGRES_URL}:${POSTGRES_PORT}/${POSTGRES_DB}"
```

Make sure you replace the `TELOXIDE_TOKEN` and `DEFAULT_TELEGRAM_ID` with valid values.

### 3. Prepare the SQLx Query Cache

Using `sqlx migrate run` will initialize your PostgreSQL database with the necessary schema and tables. Ensure that `./migrations` is present in the root directory.
Assuming you do not have a database up, run
```bash
docker compose up -d db # this starts the postgres database
sqlx migrate run # This runs all the necessary database migrations and prepares the database
cargo sqlx prepare # this prepares the offline query cache
```
You are now ready to build the application

### 4. Build and Run with Docker Compose

To build and start the services, run:

```bash
docker-compose up --build
```

This will start the following services:
- **db**: PostgreSQL database for the application
- **adminer**: Adminer for managing the PostgreSQL database (configure ports as necessary or disable entirely)
- **app**: Rust-based movement tracker application

### 5. Building and Running the Application Locally

To build and run the application locally you must set the environment variables:
- **POSTGRES_DB**
- **POSTGRES_USER**
- **POSTGRES_PASSWD**
- **POSTGRES_URL**
- **POSTGRES_PORT**

Ensure that the database is online and accepting connections such that SQLx can build against the database (without Docker):

```bash
cargo build --release
```

## Docker Compose Details

### Docker Services

- **db**: PostgreSQL database for the app.
- **adminer**: Database management interface for PostgreSQL.
- **app**: The Rust-based movement tracker application.

## Volumes

- `database_data`: Stores the PostgreSQL database data persistently.

## Configuration

### `.env` Variables

- **TELOXIDE_TOKEN**: Telegram bot token for interacting with Telegram.
- **RUST_LOG**: Controls the logging level for the application. Example: `info`, `debug`, `warn`.
- **MAX_DB_CONNECTIONS**: Maximum number of connections to the PostgreSQL database.
- **POSTGRES_DB, POSTGRES_USER, POSTGRES_PASSWD**: PostgreSQL database credentials.
- **DEFAULT_TELEGRAM_ID**: Default Telegram user ID for the bot.
- **DEFAULT_USER_NAME, DEFAULT_OPS_NAME**: Name for the default first user in the application.
- **POSTGRES_URL**, **POSTGRES_PORT**: Necessary if not running the application as a docker compose stack

