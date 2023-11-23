#include "kernel/types.h"
#include "kernel/stat.h"
#include "user/user.h"

void parent_pingpong(int pipefd[]) {
  char send_buf[] = { 0 };
  const int write_result = write(pipefd[1], send_buf, 1);
  if (write_result == -1) {
    close(pipefd[0]);
    close(pipefd[1]);
    exit(1);
  }

  sleep(1);

  char recv_buf[1];
  const int read_result = read(pipefd[0], recv_buf, 1);
  if (read_result != 1) {
    close(pipefd[0]);
    close(pipefd[1]);
    exit(1);
  }

  const int this_pid = getpid();
  fprintf(1, "%d: recieved pong\n", this_pid);

  exit(0);
}

void child_pingpong(int pipefd[]) {
  char recv_buf[1];
  const int read_result = read(pipefd[0], recv_buf, 1);
  if (read_result != 1 || recv_buf[0] != 0) {
    close(pipefd[0]);
    close(pipefd[1]);
    exit(1);
  }

  const int this_pid = getpid();
  fprintf(1, "%d: recieved ping\n", this_pid);

  char send_buf[] = { 1 };
  const int write_result = write(pipefd[1], send_buf, 1);
  if (write_result == -1) {
    close(pipefd[0]);
    close(pipefd[1]);
    exit(1);
  }
  exit(1);
}

int main(int argc, char* argv[]) {
  int pipefd[2];
  const int pipe_result = pipe(pipefd);
  if (pipe_result == -1) {
    fprintf(2, "Failed to create a pipe...\n");
    exit(1);
  }
  const int fork_result = fork();
  if (fork_result == -1) {
    fprintf(2, "Failed to fork...\n");
  } else if (fork_result != 0) {
    parent_pingpong(pipefd);
  } else {
    child_pingpong(pipefd);
  }
  exit(0);
}
