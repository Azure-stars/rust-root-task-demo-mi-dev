## 构建容器
```
make -C docker/ rm-container && make -C docker/ run && make -C docker/ exec
```

## 运行用户测例
```
make run KERNEL=<rel4/sel4>
```
