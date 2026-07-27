# PasswordManager

This project was generated using [Angular CLI](https://github.com/angular/angular-cli) version 21.2.9.

## Security & data

This is a local-only desktop password manager (Tauri + Angular). There is no server and no network sync — your data never leaves your machine.

- **Where the vault lives:** an encrypted SQLite database (`vault.db`) in the OS application-data directory, _outside_ this repository. The database and any `.env` files are git-ignored — **never commit them.**
- **Encryption:** your master password is stretched with Argon2id (m = 64 MiB, t = 3, p = 1) into a 256-bit key; every secret an entry holds — its password, notes, two-factor secret, retained previous passwords, and custom-field values — is encrypted with AES-256-GCM using a fresh random nonce. Titles, usernames, URLs, tags, and custom-field labels are stored in the clear. The master password and derived key live in memory only while the vault is unlocked and are zeroized on lock.
- **No recovery:** the master password is never stored and cannot be reset or recovered. If you forget it, the vault is unreadable — there is no backdoor.
- **Auto-lock & clipboard:** the vault auto-locks after a configurable idle timeout (30 s–24 h, default 5 min), clearing the key from memory; copied passwords are cleared from the clipboard automatically after a short delay (default 15 s).

## Development server

To start a local development server, run:

```bash
ng serve
```

Once the server is running, open your browser and navigate to `http://localhost:4200/`. The application will automatically reload whenever you modify any of the source files.

## Code scaffolding

Angular CLI includes powerful code scaffolding tools. To generate a new component, run:

```bash
ng generate component component-name
```

For a complete list of available schematics (such as `components`, `directives`, or `pipes`), run:

```bash
ng generate --help
```

## Building

To build the project run:

```bash
ng build
```

This will compile your project and store the build artifacts in the `dist/` directory. By default, the production build optimizes your application for performance and speed.

## Running unit tests

To execute unit tests with the [Vitest](https://vitest.dev/) test runner, use the following command:

```bash
ng test
```

## Running end-to-end tests

For end-to-end (e2e) testing, run:

```bash
ng e2e
```

Angular CLI does not come with an end-to-end testing framework by default. You can choose one that suits your needs.

## Additional Resources

For more information on using the Angular CLI, including detailed command references, visit the [Angular CLI Overview and Command Reference](https://angular.dev/tools/cli) page.
