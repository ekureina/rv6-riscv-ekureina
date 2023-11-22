#include "kernel/types.h"
#include "kernel/stat.h"
#include "user/user.h"

int main(int argc, char *argv[]) {
  if (argc < 2) {
    fprintf(2, "usage: sleep ms...\n");
    exit(1);
  }

  const int sleep_ms = atoi(argv[1]);

  const int result = sleep(sleep_ms);

  exit(result);
}
