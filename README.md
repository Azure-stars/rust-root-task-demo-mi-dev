## 构建容器
```
make -C docker/ rm-container && make -C docker/ run && make -C docker/ exec
```

## 运行用户测例
```
make -C docker/ rm-container && make -C docker/ run && make -C docker/ exec
```

Finally, inside the container, build and emulate a simple seL4-based system with a root task written
in Rust:

```
make run
```
