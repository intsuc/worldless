$execute store result score #value dynamic_vector store success score #access_ok dynamic_vector run data get storage dynamic_vector:state work.pages[$(page)][$(offset)]
return run execute if score #access_ok dynamic_vector matches 1
