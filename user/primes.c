#include "kernel/types.h"
#include "kernel/stat.h"
#include "user/user.h"

void subprimes(int pipefd[2]) {
  close(pipefd[1]);
  int recv_buf[1];
  const int read_result = read(pipefd[0], recv_buf, sizeof(recv_buf[0]));
  if (read_result == 0) {
    close(pipefd[0]);
    exit(0);
  } else if (read_result != sizeof(recv_buf[0])) {
    fprintf(2, "Failed to read an int from %d...\n", getpid());
    close(pipefd[0]);
    exit(1);
  }
  

  const int first_prime = recv_buf[0];
  fprintf(1, "prime %d\n", first_prime);
  const int second_read_result = read(pipefd[0], recv_buf, sizeof(recv_buf[0]));
  if (second_read_result == 0) {
    close(pipefd[0]);
    exit(0);
  } else if (second_read_result != sizeof(recv_buf[0])) {
    fprintf(2, "Failed to read an int from %d...\n", getpid());
    close(pipefd[0]);
    exit(1);
  }

  int pass_pipefd[2];
  const int pipe_result = pipe(pass_pipefd);
  if (pipe_result == -1) {
    fprintf(2, "Failed to create a pipe from %d...\n", getpid());
    close(pipefd[0]);
    exit(1);
  }

  const int fork_result = fork();
  if (fork_result == -1) {
    fprintf(2, "Failed to fork from %d...\n", getpid());
    close(pipefd[0]);
    close(pass_pipefd[0]);
    close(pass_pipefd[1]);
    exit(1);
  } else if (fork_result != 0) {
    close(pass_pipefd[0]);
    int pass_read_result = sizeof(recv_buf[0]);
    while (pass_read_result != 0) {
      if (recv_buf[0] % first_prime != 0) {
        const int pass_write_result = write(pass_pipefd[1], recv_buf, sizeof(recv_buf[0]));
        if (pass_write_result == -1) {
          fprintf(2, "Failed to write an int from %d (%d)...\n", getpid(), recv_buf[0]);
          close(pipefd[0]);
          close(pass_pipefd[1]);
          exit(1);
        }
      }

      pass_read_result = read(pipefd[0], recv_buf, sizeof(recv_buf[0]));        
      if (pass_read_result == -1) {
        fprintf(2, "Failed to read an int from %d...\n", getpid());
        close(pipefd[0]);
        close(pass_pipefd[1]);
        exit(1);
      }
    }
    close(pipefd[0]);
    close(pass_pipefd[1]);
    while (wait((int*) 1) != -1) {}
  } else {
    close(pipefd[0]);
    subprimes(pass_pipefd);
  }
}

int main(int argc, char* argv[]) {
  int pipefd[2];
  const int pipe_result = pipe(pipefd);
  if (pipe_result == -1) {
    fprintf(2, "Failed to create a pipe from %d...\n", getpid());
    exit(1);
  }
  const int fork_result = fork();
  if (fork_result == -1) {
    close(pipefd[0]);
    close(pipefd[1]);
    fprintf(2, "Failed to fork from %d...\n", getpid());
    exit(1);
  } else if (fork_result != 0) {
    close(pipefd[0]);
    int send_buf[1];
    for (int i = 2; i < 36; i++) {
      send_buf[0] = i;
      const int write_result = write(pipefd[1], send_buf, sizeof(send_buf[0]));
      if (write_result != sizeof(send_buf[0])) {
        fprintf(2, "Failed to write an int from %d...\n", getpid());
        close(pipefd[0]);
        close(pipefd[1]);
        exit(1);
      }
    }
    close(pipefd[1]);
    while (wait((int*) 1) != -1) {}
  } else {
    subprimes(pipefd);
  }
  exit(0);
}
