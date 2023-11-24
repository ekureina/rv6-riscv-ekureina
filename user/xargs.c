#include "kernel/types.h"
#include "kernel/param.h"
#include "kernel/stat.h"
#include "user/user.h"

int main(int argc, char* argv[]) {
  char* argv_copy[MAXARG];
  char new_arg_buf[512];
  char* new_arg_pointer = new_arg_buf;
  for (int i = 1; i < argc; i++) {
    argv_copy[i - 1] = argv[i];
  }

  // Null out the end of the arguments we will use
  argv_copy[argc] = 0;

  int read_result = -1;
  while (read_result != 0) {
    while ((read_result = read(0, new_arg_pointer, sizeof(new_arg_buf[0]))) >= 1) {
      if (*new_arg_pointer++ == '\n') {
        new_arg_pointer--;
        break;
      }
    }
    *new_arg_pointer = 0;
    if (strcmp(new_arg_buf, "") != 0) {
      argv_copy[argc - 1] = new_arg_buf;
      int fork_result = fork();
      if (fork_result == -1) {
        fprintf(2, "xargs: failed to fork\n");
        exit(1);
      } else if (fork_result != 0) {
        while (wait((int*) 1) != -1) {}
      } else {
        exec(argv_copy[0], argv_copy);
      }
      new_arg_pointer = new_arg_buf;
      new_arg_buf[0] = 0;
    }
  }
}
