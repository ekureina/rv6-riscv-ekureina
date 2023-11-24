#include "kernel/types.h"
#include "kernel/stat.h"
#include "user/user.h"
#include "kernel/fs.h"

char* fmtname(char* path) {
  static char buf[DIRSIZ+1];
  char *p;

  // Find first character after last slash.
  for(p=path+strlen(path); p >= path && *p != '/'; p--)
    ;
  p++;

  // Return blank-padded name.
  if(strlen(p) >= DIRSIZ)
    return p;
  memmove(buf, p, strlen(p));
  memset(buf+strlen(p), ' ', DIRSIZ-strlen(p));
  return buf;
}
void find(char* path, char* pattern);

int main(int argc, char* argv[]) {
  if (argc < 3) {
    fprintf(2, "usage: find path pattern...\n");
    exit(1);
  }

  char* path = argv[1];
  char* pattern = argv[2];
  find(path, pattern);
  exit(0);
}

void find(char* path, char* pattern) {
  char buf[512], *p;
  struct dirent de;
  struct stat st;

  if (stat(path, &st) < 0) {
    fprintf(2, "find: cannot stat %s\n", path);
    return;
  }

  switch (st.type) {
    case T_DEVICE:
    case T_FILE:
      char* start_cmp;
      // Find first character after last slash.
      for (start_cmp = path+strlen(path); start_cmp >= path && *start_cmp != '/'; start_cmp--) {}
      start_cmp++;

      if (strcmp(start_cmp, pattern) == 0) {
        printf("%s\n", path);
      }
      break;
    case T_DIR:
      if (strlen(path) + 1 + DIRSIZ + 1 > sizeof(buf)) {
        fprintf(2, "find: path too long\n");
        break;
      }
      int fd;
      if ((fd = open(path, 0)) < 0) {
        fprintf(2, "find: cannot open %s\n", path);
        return;
      }

      strcpy(buf, path);
      p = buf + strlen(buf);
      *p++ = '/';
      while (read(fd, &de, sizeof(de)) == sizeof(de)) {
        if (de.inum == 0) {
          continue;
        }
        memmove(p, de.name, DIRSIZ);
        p[DIRSIZ] = 0;
        if (strcmp(p, ".") != 0 && strcmp(p, "..") != 0) {
          find(buf, pattern);
        }
      }
      close(fd);
      break;
  }
}