## symlinkfixer

Rewrite symlinks.

### Usage
Correct usage looks like this:

```
symlinkfixer fix --old <DIR> --new <DIR> <DIRS>...
```

Any number of directories can be specified in DIRS, each of them will be scanned for symlinks. Any symlink with a prefix matching the `--old` parameter will be re-written to point into `--new` instead.
Note: currently the tool never descends into symlink'd directories, and symlinks to directories are re-written when matching same as any other symlink.