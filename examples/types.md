# How to Check Whether Object is a DataFrame
This requires using eval() which is dangerous.

```python
import pandas as pd
x = pd.DataFrame(data={'col1': [1, 2], 'col2': [3, 4]})
userInput = "x" 
evalCheck = isinstance(eval(userInput),pd.DataFrame)
print(evalCheck)
```
>> True


An alternative approach is using globals() to check the symbols.

```python
def check(userInput):
    if userInput in globals():
        return isinstance(globals()[userInput], pd.DataFrame)
    return False
localsCheck = check(userInput)
print(localsCheck)
```
>> True


Both yield the same output.
