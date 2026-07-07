When adding new features, also add examples to the `examples` directory

You can easily build and run an example by running `./run-example.sh <example-name>`. This may be useful to check the new features as well as check for regressions

The `milestone` example is the one that is allowed to fail. The language is still in early development, this example contains features that are not yet implemented. All other examples should be able to build and run successfully.

When developing new features, make sure to think is there any cases where the compiler should be able to infer types? If so, implement type inference for those cases
