#ifndef BULL_CORE_BRIDGE_H
#define BULL_CORE_BRIDGE_H

#ifdef __cplusplus
extern "C" {
#endif

char *bull_core_version_json(void);
char *bull_bridge_handle_json(const char *request_json);
void bull_bridge_free_string(char *value);

#ifdef __cplusplus
}
#endif

#endif
