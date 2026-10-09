#ifndef I2PR_SAM_H
#define I2PR_SAM_H
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
int32_t i2pr_sam_connect(const uint8_t *endpoint, size_t len, uint64_t *out_handle);
int32_t i2pr_sam_lookup(uint64_t handle, const uint8_t *name, size_t name_len,
                        uint8_t *out, size_t capacity, size_t *out_len);
int32_t i2pr_sam_close(uint64_t handle);
int32_t i2pr_sam_generate_destination(uint64_t handle,
    uint8_t *public_out, size_t public_capacity, size_t *public_len,
    uint8_t *secret_out, size_t secret_capacity, size_t *secret_len);
#ifdef __cplusplus
}
#endif
#endif
