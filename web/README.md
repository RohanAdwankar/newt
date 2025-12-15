# newtcloud
newtcloud is a cloud-based platform 
the majority of the codebase is for a webapp frontend for newt
the user ought to be able to run newtcloud in 2 ways
first, they can run the core api locally on their machine
in this configuration the hosted webapp will simply interact with the coreapi running on the users localhost
second, the user can sign up for a hosted newtcloud account
this will avoid the need for local compute / storage
it will also mean the user doesnt need to run the core api locally
this means we will eventually need to implement a new version of the core api that runs in a serverless way on next.js using vercel's services 
so the way this would work is it just uses the fact that next.js api route can run as serverless functions to run the cells in a serverless way
