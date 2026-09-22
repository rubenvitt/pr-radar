// Muss als erstes importiert werden, damit .env vor dem Auswerten anderer Module geladen ist.
try {
  process.loadEnvFile(".env");
} catch {
  /* keine .env – ok */
}
