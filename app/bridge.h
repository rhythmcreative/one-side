// The Rust core (libone_side.a): state as JSON, commands as JSON.
char *os_state(void);
char *os_command(const char *json);
void os_free(char *p);
