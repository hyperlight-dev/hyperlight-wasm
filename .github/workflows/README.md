# GitHub Actions Workflows

This directory contains [GitHub Workflows](https://docs.github.com/en/actions/using-workflows) of two primary types:

- Ones to be used as dependencies within other workflow files outside this directory.
  - These types of workflows are stored in files with names preceded with `dep_`
- Ones to be executed directly.

## More information on dependency workflows

For more information on how dependencies work in GitHub Actions, see the [GitHub documentation on reusing workflows](https://docs.github.com/en/actions/using-workflows/reusing-workflows).

### About the `workflow_call` trigger

The primary mechanism by which all files within this directory declare themselves dependencies of others is the `workflow_call` trigger. This indicates to GitHub Actions that, for a given workflow, another workflow will invoke it.

To read more about this trigger, see [GitHub Actions documentation](https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions#onworkflow_call).
