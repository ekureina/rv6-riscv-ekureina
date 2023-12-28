#ifndef SYSINFO_H
#define SYSINFO_H
#include "types.h"
struct sysinfo {
  uint64 max_mem;
  uint64 cpu_count;
  uint64 freemem;
  uint64 nproc;
};
#endif // SYSINFO_H
