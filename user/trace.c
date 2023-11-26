#include "kernel/types.h"
#include "kernel/param.h"
#include "kernel/stat.h"
#include "user/user.h"

int main(int argc, char *argv[]) {
  if (argc < 3) {
    fprintf(2, "usage: trace mask program...\n");
    exit(1);
  }

  int fork_result = fork();
  if (fork_result == -1) {
    fprintf(2, "trace: unable to fork\n");
    exit(1);
  } else if (fork_result != 0) {
    while (wait((int *) 1) != -1) {}
  } else {
    char* mask_data = argv[1];
    int parsed_mask = atoi(mask_data);
    if (trace(parsed_mask) == -1) {
      fprintf(2, "trace: unable to trace\n");
      exit(1);
    }
    char* argv_copy[MAXARG];
    for (int i = 2; i < argc; i++) {
      argv_copy[i - 2] = argv[i];
    }
    argv_copy[argc - 2] = 0;
    exec(argv_copy[0], argv_copy);
  }
  exit(0);
}
